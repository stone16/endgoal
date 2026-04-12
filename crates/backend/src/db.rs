use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// Create a SQLite connection pool.
pub async fn create_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Enable WAL mode and foreign keys
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;

    Ok(pool)
}

/// Run embedded migrations (applies the initial schema).
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Apply the initial schema inline since we don't use sqlx migrations embed
    let schema = include_str!("../../../db/migrations/001_initial_schema.sql");
    for statement in schema.split(';') {
        let trimmed = statement.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(pool).await?;
        }
    }
    Ok(())
}
