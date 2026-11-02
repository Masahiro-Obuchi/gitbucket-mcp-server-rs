use crate::error::Result;
use crate::models::user::User;

use super::client::GitBucketClient;

impl GitBucketClient {
    /// Get the authenticated user's info.
    pub async fn get_authenticated_user(&self) -> Result<User> {
        self.get("/user").await
    }

    /// Get a user by username.
    pub async fn get_user(&self, username: &str) -> Result<User> {
        super::url_safety::identifier(username, "username")?;
        self.get(&format!("/users/{}", username)).await
    }
}
