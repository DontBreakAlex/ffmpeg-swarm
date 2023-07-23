// use tokio::sync::mpsc::{Sender, Receiver};
// use tokio::sync::oneshot;

// use super::commands::DispatchedJob;

// pub enum CoordinationCommand {
//     AcquireJob {
//         reply: oneshot::Sender<DispatchedJob>
//     },
//     StoreExternalJob,
// }

// pub async fn loop_coordinator(rx: Receiver<CoordinationCommand>) {
//     loop {

//     }
// }