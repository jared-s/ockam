use chrono::{DateTime, Utc};
use ockam::{Address, Context, Result, Worker};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct RingMessage(DateTime<Utc>);

struct RingWorker {
    ctr: usize,
    next: Option<Address>,
}

impl Worker for RingWorker {
    type Context = Context;
    type Message = RingMessage;

    fn handle_message(&mut self, context: &mut Self::Context, msg: Self::Message) -> Result<()> {
        self.ctr += 1;
        if self.ctr <= 1024 {
            context
                .send_message(self.next.as_ref().unwrap().clone(), msg)
                .unwrap();
        } else {
            let now = Utc::now();
            println!(
                "Worker ring took {}ms to execute",
                (now - msg.0).num_milliseconds()
            );
            context.stop().unwrap();
        }

        Ok(())
    }
}

#[ockam::node]
async fn main(ctx: Context) {
    // Create worker state with each worker having a 'next' address
    let mut workers: Vec<RingWorker> = (0..8).fold(vec![], |mut vec, x| {
        let w = RingWorker { ctr: 0, next: None };

        if let Some(prev) = vec.last_mut() {
            prev.next = Some(format!("io.ockam.ring{}", x).into());
        }
        vec.push(w);
        vec
    });

    // Update the last worker to have the first worker as 'next'
    if let Some(last) = workers.last_mut() {
        last.next = Some(format!("io.ockam.ring0").into());
    }

    // Start all the workers
    workers.into_iter().enumerate().for_each(|(idx, worker)| {
        let addr: Address = format!("io.ockam.ring{}", idx).into();
        ctx.start_worker(addr, worker).unwrap();
    });

    // Create the first message in the system
    let msg = RingMessage(Utc::now());
    ctx.send_message("io.ockam.ring0", msg).unwrap();
}
