use crate::turtl::Turtl;
use crate::error::{TResult, TError};

/// Stores feedback we'll be sending to the server
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Debug)]
pub struct Feedback {
    pub body: String,
}

impl Feedback {
    /// Send feedback back. BO!
    pub fn send(&self, turtl: &Turtl) -> TResult<()> {
        let user_guard = lockr!(turtl.user);
        if !user_guard.logged_in {
            // nice try
            return TErr!(TError::Msg(String::from("can't send feedback, not logged in")));
        }
        turtl.api.post("/feedback")?
            .json(&self)
            .call::<bool>()?;
        Ok(())
    }
}

