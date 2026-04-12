use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://endgoal.db?mode=rwc".to_string());

    let pool = endgoal_backend::create_pool(&db_url).await?;
    endgoal_backend::run_migrations(&pool).await?;

    let app = endgoal_backend::create_router(pool);

    let addr = "0.0.0.0:3001";
    println!("Backend listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
