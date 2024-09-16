# URL Shortener in Rust

This repository contains a URL shortener service implemented in Rust, utilizing the Rocket web framework, SQLx for PostgreSQL database interactions, and Shuttle for deployment. The service allows users to shorten URLs, retrieve shortened versions, and redirect them while leveraging caching for performance optimization.

## Features

- **URL Shortening**: Submit a URL and receive a shortened version.
- **Redirect Service**: Use the shortened URL to be redirected to the original link.
- **Caching**: Caches both shortened URLs and original URLs for faster lookup.
- **Database Integration**: Stores URL mappings in a PostgreSQL database with automatic expiration of old links.
- **Auto-Cleanup**: Periodically removes expired URLs from the cache and database.
- **FIFO Cache Management**: Ensures the cache stays within size limits by pruning older entries.

## Getting Started

### Prerequisites

- **Rust**: Install Rust from [here](https://www.rust-lang.org/tools/install).
- **PostgreSQL**: Set up a PostgreSQL database locally if you want to run the service without Shuttle.
- **Shuttle**: The project is designed for deployment on [Shuttle](https://shuttle.rs/), which handles the PostgreSQL connection.

### Running Locally

If you are running the project locally without Shuttle, you’ll need to set up your environment with a proper database connection.

1. Clone the repository:

   ```bash
   git clone https://github.com/kris007iron/url-shortener.git
   cd url-shortener
   ```

2. Set up the PostgreSQL database:

   All you need is Docker engine running

3. Db config:

   Create a table thru pgAdmin in Record struct well... structure.

4. Build and run the server(loccaly):

   ```bash
   cargo-shuttle run
   ```

Now the service will be running locally, listening for URL shortening and redirection requests.

### Deployment on Shuttle

If you are deploying with Shuttle, the connection to the PostgreSQL database is handled automatically by Shuttle’s dependency injection. Shuttle will provide a connection pool directly to the `main` function, so there is **no need to set a connection string**.

To deploy the project:

1. Install the Shuttle CLI if you haven't already:

   ```bash
   cargo install shuttle-cli
   ```

2. Run the following command to deploy:

   ```bash
   cargo-shuttle project start
   ```
   or(locally as mentioned previously)
   ```bash
   cargo-shuttle run
   ```

Shuttle automatically provisions a PostgreSQL database and injects the connection pool, so no manual database setup is required.

### Endpoints

- **GET /**: Serves the homepage (HTML).
- **GET /favicon.png**: Serves the favicon image.
- **POST /**: Accepts a URL and returns its shortened version. If the URL is already shortened, the same ID is returned.
  
  Example:

  ```bash
  curl -X POST http://localhost:8000/ -d "https://example.com"
  ```

- **GET /<id>**: Redirects to the original URL associated with the shortened `id`.

### Caching and Expiration

- Cached entries expire after 24 hours and are stored in `DashMap`, which allows for quick concurrent access.
- Cache cleanup occurs automatically every hour, ensuring that expired entries are removed.
- The cache employs a **FIFO** strategy to prune older entries if the cache exceeds its maximum size.

### Database Cleanup

- Expired URLs are automatically removed from the PostgreSQL database every hour via an asynchronous cleanup task.

## Technologies Used

- **Rust**: Core programming language.
- **Rocket**: Web framework for Rust.
- **SQLx**: Async SQL toolkit for Rust, used here with PostgreSQL.
- **DashMap**: Concurrent hashmap for efficient caching.
- **Chrono**: For date and time handling, particularly expiration dates.
- **Shuttle**: Handles deployment and PostgreSQL provisioning automatically.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
