pub mod auth;
pub mod library;
pub mod stations;
pub mod streaming;
pub mod middleware;

pub use auth::auth_routes;
pub use library::library_routes;
pub use stations::station_routes;
pub use streaming::streaming_routes;
