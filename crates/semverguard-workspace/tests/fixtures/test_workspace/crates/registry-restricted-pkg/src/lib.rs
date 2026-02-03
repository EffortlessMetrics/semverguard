//! A package restricted to a specific registry.

/// A simple function.
pub fn private_api() -> &'static str {
    "This package goes to my-private-registry"
}
