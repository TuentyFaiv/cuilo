//! Model, storage, business rules and HTTP routes for example items.

mod model;
mod repository;
mod routes;
mod service;

pub use model::{ExampleItem, ExampleItemChanges, NewExampleItem};
pub use repository::ExampleItemRepository;
// @cuilo if !stack.sqlx || tests
#[cfg(test)] // @cuilo if stack.sqlx
pub use repository::InMemoryExampleItemRepository;
// @cuilo end
pub use repository::PgExampleItemRepository; // @cuilo if stack.sqlx
pub use routes::routes;
pub use service::ExampleItemService;
