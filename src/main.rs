use async_std::io;
use libp2p::{identity, mdns::{Mdns, MdnsConfig, MdnsEvent}, swarm::{Swarm, SwarmEvent}, PeerId};
use std::error::Error;
use futures::executor::block_on;
use futures::{prelude::*, select};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
    record::Key, Kademlia, KademliaEvent, PutRecordOk, QueryResult, Quorum, Record,
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

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create a random key for ourselves.
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("My Peer Id: {}", local_peer_id);

    // Set up a an encrypted DNS-enabled TCP Transport over the Mplex protocol.
    let transport = development_transport(local_key).await?;

    let client = std::env::args().skip(1).next() == Some("client".into());
    if client {
        println!("Started in CLIENT mode");
    } else {
        println!("Started in SERVER mode");
    }

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
        fn inject_event(&mut self, message: KademliaEvent) {
            match message {
                KademliaEvent::OutboundQueryCompleted { result, .. } => {
                    match result {
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
                        _ => {}
                    }
                },
                _ => {}
            }
        }
    }

    // Create a swarm to manage peers and events.
    let mut swarm = {
        // Create a Kademlia behaviour.
        let store = create_store(local_peer_id, client)?;
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
    if client {
        loop {
            select! {
                line = stdin.select_next_some() => handle_input_line(&mut swarm.behaviour_mut().kademlia,
                    line.expect("Stdin not to close"))?,
                event = swarm.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("Listening on {:?}", address);
                    },
                    _ => {}
                }
            }
        }
    } else {
        loop {
            select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {:?}", address);
                },
                _ => {}
            }
        }
        }

    }
}

fn create_store(peer_id: PeerId, client: bool) -> errors::Result<MemoryStore> {
    let mut store = MemoryStore::new(peer_id);

    if !client {
        store.put(Record::new(Key::new(&"andrew"), Vec::from("55")))?;
    }

    Ok(store)
}

/*
fn preload_store(kademlia: &mut Kademlia<MemoryStore>) -> crate::errors::Result<()> {
    let record = Record::new(Key::new(&"andrew"), Vec::from("55"));
    kademlia.put_record(record, Quorum::One )?;

    Ok(())
}
*/

fn put_record(kademlia: &mut Kademlia<MemoryStore>, key: Key, value: Vec<u8>) -> errors::Result<()> {
    let record = Record {
        key,
        value,
        publisher: None,
        expires: None,
    };
    kademlia.put_record(record, Quorum::One)?;

    Ok(())
}

fn handle_input_line(kademlia: &mut Kademlia<MemoryStore>, line: String) -> errors::Result<()> {
    let mut args = line.split(' ');

    match &args.next().map(|s| s.to_ascii_uppercase()).ok_or("Could not parse input string")? as &str {
        "GET" => {
            let key = Key::new(&args.next().ok_or("Expected key")?);
            kademlia.get_record(key, Quorum::One);
        }
        "PUT" => put_record(kademlia,
                            Key::new(&args.next().ok_or("Expected key")?),
                            args.next().ok_or("Expected value")?.as_bytes().to_vec() )?,
        _ => {
            eprintln!("expected GET or PUT");
        }
    }

    Ok(())
}