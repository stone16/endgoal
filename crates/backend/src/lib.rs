pub use endgoal_shared as shared;

mod db;
mod errors;
mod handlers;

pub use db::{create_pool, run_migrations};
pub use handlers::create_router;
