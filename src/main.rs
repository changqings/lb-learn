// Copyright 2024 changqings
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use async_trait::async_trait;
use env_logger::Env;
use log::info;
use pingora_core::services::background::background_service;
use std::{sync::Arc, time::Duration};
use structopt::StructOpt;

use pingora_core::server::configuration::Opt;
use pingora_core::server::Server;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_core::Result;
use pingora_load_balancing::{health_check, selection::RoundRobin, LoadBalancer};
use pingora_proxy::{ProxyHttp, Session};

pub struct LB(Arc<LoadBalancer<RoundRobin>>);

#[async_trait]
impl ProxyHttp for LB {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(&self, _session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        let upstream = self
            .0
            .select(b"", 256) // hash doesn't matter
            .unwrap();

        info!("upstream peer is: {:?}", upstream);

        Ok(Box::new(HttpPeer::new(upstream, false, "".to_owned())))
    }
}

// RUST_LOG=INFO cargo run
fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // read command line arguments
    let opt = Opt::from_args();
    let mut my_server = Server::new(Some(opt)).unwrap();
    my_server.bootstrap();

    // 127.0.0.1:83 is not health, just to show health_check work
    let mut upstreams =
        LoadBalancer::try_from_iter(["127.0.0.1:81", "127.0.0.1:82", "127.0.0.1:83"]).unwrap();

    // We add health check in the background so that the bad server is never selected.
    // there is also httpHealthCheck if you want use /health_check
    let hc = health_check::TcpHealthCheck::new();
    upstreams.set_health_check(hc);
    upstreams.health_check_frequency = Some(Duration::from_secs(2));

    let background = background_service("health_check", upstreams);

    let upstreams = background.task();

    let mut lb = pingora_proxy::http_proxy_service(&my_server.configuration, LB(upstreams));
    lb.add_tcp("0.0.0.0:80");

    my_server.add_service(lb);
    my_server.add_service(background);
    my_server.run_forever();
}
