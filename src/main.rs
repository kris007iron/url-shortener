use chrono::{DateTime, Utc};
use dashmap::DashMap;
use nanoid;
use rocket::fs::NamedFile;
use rocket::{get, http::Status, post, response::Redirect, routes, State};
use sqlx::{Error, PgPool};
use std::path::Path;

use tokio::time::{interval, Duration as TokioDuration};

#[derive(Clone)]
struct Record {
    _id: String,
    _url: String,
    _expiration_date: DateTime<Utc>,
}

type CacheById = DashMap<String, Record>; // For id -> Record
type CacheByUrl = DashMap<String, Record>; // For url -> Record

#[get("/")]
async fn index() -> Option<NamedFile> {
    NamedFile::open(Path::new("src/frontend/index.html"))
        .await
        .ok()
}
#[get("/favicon.png")]
async fn favicon() -> Option<NamedFile> {
    NamedFile::open(Path::new("src/frontend/favicon.png"))
        .await
        .ok()
}

#[get("/<id>")]
async fn redirect(
    id: String,
    pool: &State<PgPool>,
    cache_by_id: &State<CacheById>, // Use the id->Record cache
) -> Result<Redirect, Status> {
    // Check if the URL is in the cache (by id)
    if let Some(record) = cache_by_id.get(&id) {
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
    cache_by_id.insert(id.clone(), record);

    Ok(Redirect::to(url.0))
}

#[post("/", data = "<url>")]
async fn shorten(
    url: String,
    pool: &State<PgPool>,
    cache_by_id: &State<CacheById>,   // Use the id->Record cache
    cache_by_url: &State<CacheByUrl>, // Use the url->Record cache
) -> Result<String, Status> {
    // Check if URL is in the cache first (by url)
    if let Some(record) = cache_by_url.get(&url) {
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
        cache_by_id.insert(id.0.clone(), record.clone()); // Cache by id
        cache_by_url.insert(url.clone(), record); // Cache by url

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
    cache_by_id.insert(id.clone(), record.clone()); // Cache by id
    cache_by_url.insert(url.clone(), record); // Cache by url

    Ok(format!("https://shortrl.shuttleapp.rs/{}", id))
}

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



#[shuttle_runtime::main]
async fn main(#[shuttle_shared_db::Postgres] _pool: PgPool) -> shuttle_rocket::ShuttleRocket {
    let cache_by_id: CacheById = DashMap::new();
    let cache_by_url: CacheByUrl = DashMap::new();

    tokio::spawn(delete_expired_urls(_pool.clone()));
    let rocket = rocket::build()
        .mount("/", routes![index, favicon])
        .mount("/", routes![redirect, shorten])
        .manage(cache_by_id)
        .manage(cache_by_url)
        .manage(_pool);
    Ok(rocket.into())
}
