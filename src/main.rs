//! A basic key value store demonstrating libp2p and the mDNS and Kademlia protocols.
//!
//! 1. Using two terminal windows, start two instances. If you local network
//!    allows mDNS, they will automatically connect.
//!
//! 2. Type `PUT my-key my-value` in terminal one and hit return.
//!
//! 3. Type `GET my-key` in terminal two and hit return.
//!
//! 4. Close with Ctrl-c.
//!
//! You can also store provider records instead of key value records.
//!
//! 1. Using two terminal windows, start two instances. If you local network
//!    allows mDNS, they will automatically connect.
//!
//! 2. Type `PUT_PROVIDER my-key` in terminal one and hit return.
//!
//! 3. Type `GET_PROVIDERS my-key` in terminal two and hit return.
//!
//! 4. Close with Ctrl-c.

use async_std::io;
use libp2p::{identity, mdns::{Mdns, MdnsConfig, MdnsEvent}, swarm::{Swarm, SwarmEvent}, PeerId};
use std::error::Error;
use futures::executor::block_on;
use futures::{prelude::*, select};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
    record::Key, AddProviderOk, Kademlia, KademliaEvent, PutRecordOk, QueryResult, Quorum, Record,
};
use libp2p::{
    development_transport,
    swarm::{NetworkBehaviourEventProcess},
    NetworkBehaviour,
};
use libp2p::kad::store::RecordStore;

/// We'll put our errors in an `errors` module, and other modules in this crate will
/// `use crate::errors::*;` to get access to everything `error_chain` creates.
pub mod errors;

use crate::errors::bail;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create a random key for ourselves.
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("My Peer Id: {}", local_peer_id);

    // Set up a an encrypted DNS-enabled TCP Transport over the Mplex protocol.
    let transport = development_transport(local_key).await?;

    // We create a custom network behaviour that combines Kademlia and mDNS.
    #[derive(NetworkBehaviour)]
    #[behaviour(event_process = true)]
    struct MyBehaviour {
        kademlia: Kademlia<MemoryStore>,
        mdns: Mdns,
    }

    impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
        // Called when `mdns` produces an event.
        fn inject_event(&mut self, event: MdnsEvent) {
            if let MdnsEvent::Discovered(list) = event {
                for (peer_id, multiaddr) in list {
                    println!("New Peer '{}' at {} added to network", peer_id, multiaddr);
                    self.kademlia.add_address(&peer_id, multiaddr);
                }
            }
        }
    }

    impl NetworkBehaviourEventProcess<KademliaEvent> for MyBehaviour {
        // Called when `kademlia` produces an event.
        fn inject_event(&mut self, message: KademliaEvent) {
            match message {
                KademliaEvent::OutboundQueryCompleted { result, .. } => {
                    match result {
                        QueryResult::GetProviders(Ok(ok)) => {
                            println!("Get Providers OK");
                            for peer in ok.providers {
                                println!(
                                    "Peer {:?} provides key {:?}",
                                    peer,
                                    std::str::from_utf8(ok.key.as_ref()).unwrap()
                                );
                            }
                        }
                        QueryResult::GetProviders(Err(err)) => {
                            eprintln!("Failed to get providers: {:?}", err);
                        }
                        QueryResult::GetRecord(Ok(ok)) => {
                            for peer_record in ok.records
                            {
                                println!(
                                    "Got record {:?} {:?} from peer {:?}",
                                    std::str::from_utf8(peer_record.record.key.as_ref()).unwrap(),
                                    std::str::from_utf8(&peer_record.record.value).unwrap(),
                                    peer_record.peer
                                );
                            }
                        }
                        QueryResult::GetRecord(Err(err)) => {
                            eprintln!("Failed to get record: {:?}", err);
                        }
                        QueryResult::PutRecord(Ok(PutRecordOk { key })) => {
                            println!(
                                "Successfully put record {:?}",
                                std::str::from_utf8(key.as_ref()).unwrap()
                            );
                        }
                        QueryResult::PutRecord(Err(err)) => {
                            eprintln!("Failed to put record: {:?}", err);
                        }
                        QueryResult::StartProviding(Ok(AddProviderOk { key })) => {
                            println!(
                                "Successfully put provider record {:?}",
                                std::str::from_utf8(key.as_ref()).unwrap()
                            );
                        }
                        QueryResult::StartProviding(Err(err)) => {
                            eprintln!("Failed to put provider record: {:?}", err);
                        }
                        e => println!("Other Event: {:?}", e),
                    }
                },
                KademliaEvent::InboundRequest{ request } => println!("Inbound Request: {:?}", request),
                KademliaEvent::RoutingUpdated { .. } => println!("Routing updated"),
                KademliaEvent::UnroutablePeer { .. } => println!("Unroutable peer"),
                KademliaEvent::RoutablePeer { .. } => println!("Routable peer"),
                KademliaEvent::PendingRoutablePeer { .. } => println!("Pending routable peer"),
            }
        }
    }

    // Create a swarm to manage peers and events.
    let mut swarm = {
        // Create a Kademlia behaviour.
        let mut store = MemoryStore::new(local_peer_id);
        let _ = store.put(Record::new(Key::new(&"andrew"), Vec::from("55")));
        let kademlia = Kademlia::new(local_peer_id, store);
        let mdns = block_on(Mdns::new(MdnsConfig::default()))?;
        let behaviour = MyBehaviour { kademlia, mdns };
        Swarm::new(transport, behaviour, local_peer_id)
    };

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

    // Listen on all interfaces and whatever port the OS assigns.
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // loop - processing commands from stdin or events from the network
    loop {
        select! {
            line = stdin.select_next_some() => handle_input_line(&mut swarm.behaviour_mut().kademlia, line.expect("Stdin not to close"))?,
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {:?}", address);
                },
                _ => {}
            }
        }
    }
}

fn handle_input_line(kademlia: &mut Kademlia<MemoryStore>, line: String) -> crate::errors::Result<()> {
    let mut args = line.split(' ');

    match &args.next().map(|s| s.to_ascii_uppercase()).ok_or("Could not parse input string")? as &str {
        "GET" => {
            let key = Key::new(&args.next().ok_or("Expected key")?);
            kademlia.get_record(key, Quorum::One);
        }
        "GET_PROVIDERS" => {
            let key = Key::new(&args.next().ok_or("Expected key")?);
            kademlia.get_providers(key);
        }
        "PUT" => {
            let key = Key::new(&args.next().ok_or("Expected key")?);
            let value = args.next().ok_or("Expected value")?.as_bytes().to_vec();
            let record = Record {
                key,
                value,
                publisher: None,
                expires: None,
            };
            kademlia
                .put_record(record, Quorum::One)
                .expect("Failed to store record locally.");
        }
        "PUT_PROVIDER" => {
            let key = Key::new(&args.next().ok_or("Expected key")?);
            kademlia
                .start_providing(key)
                .expect("Failed to start providing key");
        }
        _ => {
            eprintln!("expected GET, GET_PROVIDERS, PUT or PUT_PROVIDER");
        }
    }

    Ok(())
}