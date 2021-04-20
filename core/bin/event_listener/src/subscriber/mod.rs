// Built-in uses
use std::convert::TryFrom;
// External uses
use actix::prelude::*;
use actix_web_actors::ws;
// Workspace uses
// Local uses
use crate::messages::{NewEvents, RegisterSubscriber, RemoveSubscriber};
use crate::monitor::ServerMonitor;
use filters::SubscriberFilters;

mod filters;

#[derive(Debug)]
pub struct Subscriber {
    filters: Option<SubscriberFilters>,
    monitor: Addr<ServerMonitor>,
}

impl Subscriber {
    pub fn new(monitor: Addr<ServerMonitor>) -> Self {
        Self {
            filters: None,
            monitor,
        }
    }
}

impl Actor for Subscriber {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(1 << 10);

        let request = RegisterSubscriber(ctx.address());
        self.monitor
            .send(request)
            .into_actor(self)
            .map(|response, _, ctx| {
                if let Err(err) = response {
                    vlog::error!("Couldn't register new subscriber, reason: {:?}", err);
                    ctx.stop();
                }
            })
            .wait(ctx);
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        let request = RemoveSubscriber(ctx.address());
        self.monitor
            .send(request)
            .into_actor(self)
            .map(|response, _, _| {
                if let Err(err) = response {
                    vlog::error!("Couldn't remove the subscriber, reason: {:?}", err);
                }
            })
            .wait(ctx);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Subscriber {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Text(text)) => {
                if self.filters.is_none() {
                    match SubscriberFilters::try_from(text) {
                        Ok(filters) => {
                            eprintln!("New sub events: {:?}", filters);
                            self.filters = Some(filters);
                        }
                        Err(_) => {
                            ctx.stop();
                        }
                    };
                }
            }
            Ok(ws::Message::Close(_)) => {
                ctx.stop();
            }
            _ => {}
        }
    }
}

impl Handler<NewEvents> for Subscriber {
    type Result = ();

    fn handle(&mut self, msg: NewEvents, ctx: &mut Self::Context) {
        let filters = match &self.filters {
            Some(filters) => filters,
            None => return,
        };
        for event in msg.0.as_ref() {
            if !filters.matches(event) {
                continue;
            }
            let json = serde_json::to_string(&event).unwrap();
            ctx.text(json);
        }
    }
}
