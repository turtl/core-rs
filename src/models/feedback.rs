use ::jedi;
use ::turtl::Turtl;
use ::error::{TResult, TError};
use ::api::ApiReq;

/// Stores feedback we'll be sending to the server
#[derive(Serialize, Deserialize, Debug)]
pub struct Feedback {
    pub body: String,
}

impl Feedback {
    /// Send feedback back. BO!
    pub fn send(&self, turtl: &Turtl) -> TResult<()> {
        let user_guard = turtl.user.read().unwrap();
        if !user_guard.logged_in {
            // nice try
            return Err(TError::Msg(String::from("Feedback.send() -- can't send feedback, not logged in. SHAME! SHAME, SHAME.")));
        }
        let mut req = ApiReq::new();
        req = req.data(jedi::to_val(self)?);
        turtl.api.post::<bool>("/feedback", req)?;
        Ok(())
    }
}

