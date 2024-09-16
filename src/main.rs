use chrono::{DateTime, Utc};
use dashmap::DashMap;
use nanoid;
use rocket::fs::NamedFile;
use rocket::{get, http::Status, post, response::Redirect, routes, State};
use sqlx::{Error, PgPool};
use std::path::Path;
use std::sync::Arc;

use tokio::time::{interval, Duration as TokioDuration};

/// Represents a record for storing URL mappings with an expiration time.
#[derive(Clone)]
struct Record {
    _id: String,
    _url: String,
    _expiration_date: DateTime<Utc>,
}

/// A cache structure for managing URL mappings. This cache allows fast lookups
/// for shortened URLs and their corresponding full URLs, while maintaining
/// expiration dates to clear out old entries.
struct Cache {
    /// Cache mapping IDs to full URL records.
    cache_by_id: DashMap<String, Record>,
    /// Cache mapping full URLs to shortened ID records.
    cache_by_url: DashMap<String, Record>,
}

/// Serves the index page (HTML file) to the user when they visit the root URL.
///
/// # Returns
/// - The `index.html` file if it exists.
/// - `None` if the file is not found.
#[get("/")]
async fn index() -> Option<NamedFile> {
    NamedFile::open(Path::new("src/frontend/index.html"))
        .await
        .ok()
}

/// Serves the favicon image for the web page.
///
/// # Returns
/// - The `favicon.png` file if it exists.
/// - `None` if the file is not found.
#[get("/favicon.png")]
async fn favicon() -> Option<NamedFile> {
    NamedFile::open(Path::new("src/frontend/favicon.png"))
        .await
        .ok()
}

/// Redirects a user to the full URL based on the provided shortened ID.
/// It checks the cache first, and if not found, queries the database.
///
/// # Arguments
/// - `id`: The shortened ID from the URL path.
/// - `pool`: The database connection pool provided by Rocket.
/// - `cache`: The cache state containing URL mappings.
///
/// # Returns
/// - A `Redirect` to the full URL if found.
/// - A `Status::NotFound` if the ID is not found.
/// - A `Status::InternalServerError` if a database query fails.
///
/// # Example
/// Visiting `http://shortrl.shuttleapp.rs/12345a` will redirect the user
/// to the full URL associated with the ID `12345a`.
#[get("/<id>")]
async fn redirect(
    id: String,
    pool: &State<PgPool>,
    cache: &State<Arc<Cache>>,
) -> Result<Redirect, Status> {
    // Check if the URL is in the cache (by id)
    if let Some(record) = cache.cache_by_id.get(&id) {
        // If found in cache, redirect to the cached URL
        return Ok(Redirect::to(record._url.clone()));
    }

    // If not found in cache, query the database
    let url: (String,) = match sqlx::query_as("SELECT url FROM urls WHERE id = $1")
        .bind(&id)
        .fetch_one(&**pool)
        .await
    {
        Ok(result) => result,
        Err(Error::RowNotFound) => return Err(Status::NotFound),
        Err(_) => return Err(Status::InternalServerError),
    };

    // Cache the result for future requests (by id)
    let expiration_date = Utc::now() + chrono::Duration::hours(24);
    let record = Record {
        _id: id.clone(),
        _url: url.0.clone(),
        _expiration_date: expiration_date,
    };
    cache.cache_by_id.insert(id.clone(), record);

    Ok(Redirect::to(url.0))
}

/// Creates or returns a shortened URL for the provided full URL.
/// If the URL has already been shortened, the existing shortened URL is returned.
/// Otherwise, a new shortened URL is generated and stored in the database and cache.
///
/// # Arguments
/// - `url`: The full URL to shorten.
/// - `pool`: The database connection pool provided by Rocket.
/// - `cache`: The cache state containing URL mappings.
///
/// # Returns
/// - The shortened URL if successful.
/// - A `Status::InternalServerError` if the database operation fails.
///
/// # Example
/// Sending a POST request to `/` with the URL `https://example.com` will return
/// a shortened URL like `https://shortrl.shuttleapp.rs/abcd1234`.
#[post("/", data = "<url>")]
async fn shorten(
    url: String,
    pool: &State<PgPool>,
    cache: &State<Arc<Cache>>,
) -> Result<String, Status> {
    // Check if URL is in the cache first (by url)
    if let Some(record) = cache.cache_by_url.get(&url) {
        // If found in cache, return the cached shortened URL
        return Ok(format!("https://shortrl.shuttleapp.rs/{}", record._id));
    }

    // Check if the URL exists in the database
    let is_duplicate: (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM urls WHERE url = $1)")
        .bind(&url)
        .fetch_one(&**pool)
        .await
        .map_err(|_| Status::InternalServerError)?;

    if is_duplicate.0 {
        // If the URL already exists, fetch its ID from the database
        let id: (String,) = sqlx::query_as("SELECT id FROM urls WHERE url = $1")
            .bind(&url)
            .fetch_one(&**pool)
            .await
            .map_err(|_| Status::InternalServerError)?;

        // Cache the record after fetching from the database (by id and by url)
        let expiration_date = Utc::now() + chrono::Duration::hours(24);
        let record = Record {
            _id: id.0.clone(),
            _url: url.clone(),
            _expiration_date: expiration_date,
        };
        cache.cache_by_id.insert(id.0.clone(), record.clone()); // Cache by id
        cache.cache_by_url.insert(url.clone(), record); // Cache by url

        return Ok(format!("https://shortrl.shuttleapp.rs/{}", id.0));
    }

    // If the URL doesn't exist, insert it into the database
    let id = nanoid::nanoid!(10);
    let expiration_date = Utc::now() + chrono::Duration::hours(24);
    sqlx::query("INSERT INTO urls(id, url, expiration_date) VALUES ($1, $2, $3)")
        .bind(&id)
        .bind(&url)
        .bind(expiration_date)
        .execute(&**pool)
        .await
        .map_err(|_| Status::InternalServerError)?;

    // Insert the new record into both caches (by id and by url)
    let record = Record {
        _id: id.clone(),
        _url: url.clone(),
        _expiration_date: expiration_date,
    };
    cache.cache_by_id.insert(id.clone(), record.clone()); // Cache by id
    cache.cache_by_url.insert(url.clone(), record); // Cache by url

    Ok(format!("https://shortrl.shuttleapp.rs/{}", id))
}

/// Periodically deletes expired URLs from the database based on their expiration date.
/// Runs every hour in the background.
///
/// # Arguments
/// - `pool`: The database connection pool used for the deletion query.
async fn delete_expired_urls(pool: PgPool) {
    let mut interval = interval(TokioDuration::from_secs(3600));
    loop {
        interval.tick().await;
        let result = sqlx::query("DELETE FROM urls WHERE expiration_date < $1")
            .bind(Utc::now())
            .execute(&pool)
            .await;
        if let Err(_) = result {
            eprintln!("Failed to delete expired URLs");
        }
    }
}

/// Periodically cleans up expired entries from the cache based on their expiration dates.
/// Also manages cache size by removing the oldest entries if the cache grows too large.
///
/// # Arguments
/// - `cache`: The shared cache containing URL mappings.
async fn clean_cache(cache: Arc<Cache>) {
    let mut interval = interval(TokioDuration::from_secs(3600)); // Run cleanup every 10 minutes
    loop {
        println!("Cache by id size = {}", cache.cache_by_id.capacity());
        println!("Cache by url size = {}", cache.cache_by_url.capacity());
        interval.tick().await;
        let now = Utc::now();

        // Clean up expired records from cache_by_id
        cache
            .cache_by_id
            .retain(|_, record| record._expiration_date > now);

        // Clean up expired records from cache_by_url
        cache
            .cache_by_url
            .retain(|_, record| record._expiration_date > now);

        prune_cache_if_needed(&cache);
    }
}

/// Prunes the cache if its size exceeds a predefined maximum limit (FIFO strategy).
///
/// # Arguments
/// - `cache`: The cache structure to be pruned.
fn prune_cache_if_needed(cache: &Cache) {
    const CACHE_MAX_SIZE: usize = 100;

    if cache.cache_by_id.len() > CACHE_MAX_SIZE {
        // Find and remove the oldest record in cache_by_id
        let mut oldest_key: Option<String> = None;
        let mut oldest_expiration = Utc::now();

        // Iterate to find the oldest record
        for entry in cache.cache_by_id.iter() {
            if entry.value()._expiration_date < oldest_expiration {
                oldest_expiration = entry.value()._expiration_date;
                oldest_key = Some(entry.key().clone());
            }
        }

        // Remove the oldest record if found
        if let Some(key) = oldest_key {
            cache.cache_by_id.remove(&key);
            // Also remove from cache_by_url by matching the ID
            cache.cache_by_url.retain(|_, record| record._id != key);
        }
    }

    if cache.cache_by_url.len() > CACHE_MAX_SIZE {
        let mut oldest_key: Option<String> = None;
        let mut oldest_expiration = Utc::now();

        for entry in cache.cache_by_url.iter() {
            if entry.value()._expiration_date < oldest_expiration {
                oldest_expiration = entry.value()._expiration_date;
                oldest_key = Some(entry.key().clone());
            }
        }

        if let Some(key) = oldest_key {
            cache.cache_by_url.remove(&key);
            cache.cache_by_url.retain(|_, record| record._id != key);
        }
    }
}

/// Main function that is an entry poin and runs web server and cleaning in background
#[shuttle_runtime::main]
async fn main(#[shuttle_shared_db::Postgres] _pool: PgPool) -> shuttle_rocket::ShuttleRocket {
    let cache = Arc::new(Cache {
        cache_by_id: DashMap::new(),
        cache_by_url: DashMap::new(),
    });

    tokio::spawn(clean_cache(Arc::clone(&cache)));

    tokio::spawn(delete_expired_urls(_pool.clone()));
    let rocket = rocket::build()
        .mount("/", routes![index, favicon])
        .mount("/", routes![redirect, shorten])
        .manage(cache)
        .manage(_pool);
    Ok(rocket.into())
}
