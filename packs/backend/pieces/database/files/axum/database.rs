//! The PostgreSQL connection pool that repositories share.

use sqlx::PgPool;

/// Connects a pool to the PostgreSQL server at `DATABASE_URL`.
pub async fn connect() -> Result<PgPool, sqlx::Error> {
	let url = std::env::var("DATABASE_URL")
		.map_err(|error| sqlx::Error::Configuration(format!("DATABASE_URL: {error}").into()))?;
	PgPool::connect(&url).await
}
