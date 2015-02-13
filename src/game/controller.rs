use std::error::FromError;
use std::sync::{Arc, mpsc};

use actor;
use game::{State, Action};

pub type Request = (Arc<actor::State>, Arc<State>);
pub type Reply = (Arc<actor::State>, Action);


/// Controller between actors behavior engines and `game::State`
///
/// Meant to be running in it's own thread
pub struct Controller {
    state : Arc<State>,
}

/// Possible errors that could terminate Controller
pub enum Error {
    Receive(mpsc::RecvError),
    Transmit(mpsc::SendError<Request>),
}

impl FromError<mpsc::RecvError> for Error {
    fn from_error(err: mpsc::RecvError) -> Error {
        Error::Receive(err)
    }
}

impl FromError<mpsc::SendError<Request>> for Error {
    fn from_error(err: mpsc::SendError<Request>) -> Error {
        Error::Transmit(err)
    }
}

impl Controller {
    pub fn new(state : State) -> Controller {
        Controller {
            state: Arc::new(state),
        }
    }

    pub fn run(&mut self,
               pl_req : mpsc::Sender<Request>,
               pl_rep : mpsc::Receiver<Reply>,
               ai_req : mpsc::Sender<Request>,
               ai_rep : mpsc::Receiver<Reply>,
               ) -> Result<(), Error>
    {
        loop {
            let actors = self.state.actors.clone();

            for (_, actor) in actors.iter() {
                match actor.behavior {
                    actor::Behavior::Player => {
                        try!(pl_req.send((actor.clone(), self.state.clone())));
                    },
                    actor::Behavior::Grue|actor::Behavior::Pony => {
                        try!(ai_req.send((actor.clone(), self.state.clone())));
                    },
                }
            }

            for (_, astate) in actors.iter() {
                let (astate, action) = match astate.behavior {
                    actor::Behavior::Player => {
                        try!(pl_rep.recv())
                    },
                    actor::Behavior::Grue|actor::Behavior::Pony => {
                        try!(ai_rep.recv())
                    },
                };

                self.state = Arc::new(self.state.act(&astate, action));
            }

            self.state = Arc::new(self.state.tick());
        }
    }

}
