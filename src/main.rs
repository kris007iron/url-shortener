use chrono::{DateTime, Duration, Utc};
use nanoid;
use rocket::fs::NamedFile;
use rocket::{get, http::Status, post, response::Redirect, routes, State};
use sqlx::{Error, PgPool};
use std::path::Path;

use tokio::time::{interval, Duration as TokioDuration};
use url::Url;

#[get("/")]
async fn index() -> Option<NamedFile> {
    NamedFile::open(Path::new("src/frontend/index.html"))
        .await
        .ok()
}

#[get("/<id>")]
async fn redirect(id: String, pool: &State<PgPool>) -> Result<Redirect, Status> {
    let url: (String,) = match sqlx::query_as("SELECT url FROM urls WHERE id = $1")
        .bind(id)
        .fetch_one(&**pool)
        .await
    {
        Ok(result) => result,
        Err(Error::RowNotFound) => return Err(Status::NotFound),
        Err(_) => return Err(Status::InternalServerError),
    };
    Ok(Redirect::to(url.0))
}

#[post("/", data = "<url>")]
async fn shorten(url: String, pool: &State<PgPool>) -> Result<String, Status> {
    //generate random integer from 6 to 20

    let id = &nanoid::nanoid!(21);
    let p_url = match Url::parse(&url) {
        Ok(url) => url,
        Err(_) => return Err(Status::UnprocessableEntity),
    };
    let is_duplicate: (bool,) =
        match sqlx::query_as("SELECT EXISTS(SELECT 1 FROM urls WHERE url = $1)")
            .bind(p_url.as_str())
            .fetch_one(&**pool)
            .await
        {
            Ok(result) => result,
            Err(_) => return Err(Status::InternalServerError),
        };
    if is_duplicate.0 {
        let id: (String,) = match sqlx::query_as("SELECT id FROM urls WHERE url = $1")
            .bind(p_url.as_str())
            .fetch_one(&**pool)
            .await
        {
            Ok(result) => result,
            Err(_) => return Err(Status::InternalServerError),
        };
        let expiration_date: DateTime<Utc> = DateTime::from(Utc::now() + Duration::hours(24));
        match sqlx::query("UPDATE urls SET expiration_date = $1 WHERE id = $2")
            .bind(expiration_date)
            .bind(&id.0)
            .execute(&**pool)
            .await
        {
            Ok(_) => {}
            Err(_) => return Err(Status::InternalServerError),
        };
        return Ok(format!("https://shortrl.shuttleapp.rs/{id}", id = id.0));
    } else {
        let expiration_date: DateTime<Utc> = DateTime::from(Utc::now() + Duration::hours(24));
        match sqlx::query("INSERT INTO urls(id, url, expiration_date) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(p_url.as_str())
            .bind(expiration_date) //test it cuz there are null values in db
            .execute(&**pool)
            .await
        {
            Ok(_) => {}
            Err(_) => return Err(Status::InternalServerError),
        };
        Ok(format!("https://shortrl.shuttleapp.rs/{id}"))
    }
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
    tokio::spawn(delete_expired_urls(_pool.clone()));
    let rocket = rocket::build()
        .mount("/", routes![index])
        .mount("/", routes![redirect, shorten])
        .manage(_pool);
    Ok(rocket.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::Status;
    use rocket::local::asynchronous::Client;
    use sqlx::PgPool;

    #[rocket::async_test]
    async fn test_invalid_url() {
        let pool = PgPool::connect(
            std::env::var("CONN_STRING")
                .expect("CONN_STRING not set")
                .as_str(),
        )
        .await
        .unwrap();
        let client = Client::tracked(rocket::build().manage(pool)).await.unwrap();

        let response = client.post("/").body("invalid-url").dispatch().await;

        assert_eq!(response.status(), Status::UnprocessableEntity);
    }
}
