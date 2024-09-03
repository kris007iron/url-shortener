use nanoid;
use rocket::fs::NamedFile;
use rocket::{get, http::Status, post, response::Redirect, routes, State};
use sqlx::{Error, PgPool};
use std::path::Path;
use url::Url;

#[get("/")]
async fn index() -> Option<NamedFile> {
    NamedFile::open(Path::new("frontend/index.html")).await.ok()
}

// Serve the 1712.html file when accessing "/1712"
#[get("/1712")]
async fn special_page() -> Option<NamedFile> {
    NamedFile::open(Path::new("frontend/1712.html")).await.ok()
}
#[get("/<id>")]
async fn redirect(id: String, pool: &State<PgPool>) -> Result<Redirect, Status> {
    let url: (String,) = sqlx::query_as("SELECT url FROM urls WHERE id = $1")
        .bind(id)
        .fetch_one(&**pool)
        .await
        .map_err(|e| match e {
            Error::RowNotFound => Status::NotFound,
            _ => Status::InternalServerError,
        })?;
    Ok(Redirect::to(url.0))
}

#[post("/", data = "<url>")]
async fn shorten(url: String, pool: &State<PgPool>) -> Result<String, Status> {
    let id = &nanoid::nanoid!(6);
    let p_url = Url::parse(&url).map_err(|_| Status::UnprocessableEntity)?;
    sqlx::query("INSERT INTO urls(id, url) VALUES ($1, $2)")
        .bind(id)
        .bind(p_url.as_str())
        .execute(&**pool)
        .await
        .map_err(|_| Status::InternalServerError)?;
    Ok(format!("https://shortrl.shuttleapp.rs/{id}"))
}

#[shuttle_runtime::main]
async fn main(#[shuttle_shared_db::Postgres] _pool: PgPool) -> shuttle_rocket::ShuttleRocket {
    let rocket = rocket::build()
        .mount("/", routes![index, special_page])
        .mount("/", routes![redirect, shorten])
        .manage(_pool);
    Ok(rocket.into())
}
