// Part of Distant Horizons for Horizon Client, LGPL-3.0 (see runtime.rs).

#[path = "@MIXINS@/runtime.rs"]
#[allow(dead_code)]
mod distant_horizons;
#[path = "@MIXINS@/codec.rs"]
#[allow(dead_code)]
mod distant_horizons_codec;
#[path = "@MIXINS@/net.rs"]
#[allow(dead_code)]
mod distant_horizons_net;

use distant_horizons::pos;
use distant_horizons_net as net;
use horizon_client::distant_horizons_inbox as inbox;
use horizon_client::net::{Event, Session};
use horizon_protocol::packets::{configuration::CustomPayload, play};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn send(session: &Session, channel: &str, data: Vec<u8>) {
    session.send(play::Serverbound::CustomPayload(CustomPayload {
        channel: channel.into(),
        data,
    }));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let address = args
        .iter()
        .position(|a| a == "--address")
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| "127.0.0.1:25565".to_owned());
    let seconds: u64 = std::env::var("DH_INTEROP_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(90);
    let language = Arc::new(Default::default());
    let mut session = Session::connect(&address, "DhInterop", None, 8, language);
    let mut client = net::Client::new(net::SessionConfig::default());

    let start = Instant::now();
    let mut joined = false;
    let mut registered = false;
    let mut player: Option<[f64; 2]> = None;
    let mut asked = false;
    let mut synced: Option<Vec<i64>> = None;
    let mut wanted: Vec<i64> = Vec::new();
    let (mut sources, mut empty, mut points, mut up_to_date, mut gone) = (0, 0, 0usize, 0, 0);
    let mut names: BTreeMap<String, u32> = BTreeMap::new();
    let mut failed = 0;
    let mut level_named_at = None;

    while start.elapsed() < Duration::from_secs(seconds) {
        let now = Instant::now();
        for event in session.poll() {
            match event {
                Event::Joined { name, .. } => {
                    println!("joined as {name}");
                    joined = true;
                }
                Event::Packet(packet) => {
                    if let play::Clientbound::CustomPayload(c) = &*packet {
                        if c.channel == net::CHANNEL {
                            eprintln!("FAIL: a Distant Horizons message got past the poll hook");
                            std::process::exit(2);
                        }
                    }
                    if let play::Clientbound::PlayerPosition(p) = &*packet {
                        if player.is_none() {
                            println!("at {:?}", p.position);
                        }
                        player = Some([p.position[0], p.position[2]]);
                    }
                }
                Event::Disconnected { reason, .. } => {
                    eprintln!("disconnected: {reason}");
                    std::process::exit(1);
                }
                _ => {}
            }
        }
        if !joined {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        for data in inbox::take() {
            if let Err(e) = client.receive(&data, now) {
                eprintln!("unreadable message: {e}");
                failed += 1;
            }
        }
        client.set_dimension("minecraft:overworld", now);
        if level_named_at.is_none() && client.level_name(now).is_some() {
            level_named_at = Some(start.elapsed());
            println!(
                "level: {:?} (server key {:?}) after {:.1} s",
                client.level_name(now),
                client.level().map(|l| l.server_key.clone()),
                start.elapsed().as_secs_f32()
            );
        }
        if let Some([x, z]) = player {
            if !asked && client.level_name(now).is_some() {
                asked = true;
                let (sx, sz) = ((x as i32).div_euclid(64), (z as i32).div_euclid(64));
                for dx in -2..=2 {
                    for dz in -2..=2 {
                        wanted.push(pos::encode(6, sx + dx, sz + dz));
                    }
                }
                wanted.push(pos::encode(8, sx.div_euclid(4), sz.div_euclid(4)));
                for &s in &wanted {
                    client.want(s, None, now);
                }
                println!("asked for 26 sections around ({x:.0}, {z:.0})");
            }
            client.tick(now, [x, z]);
        }
        for event in client.take_events() {
            match event {
                net::Event::Data { bytes, requested, sync } => {
                    let mut resolve = |biome: &str, block: &str| {
                        *names.entry(format!("{biome} | {block}")).or_default() += 1;
                        (0u16, 1u32)
                    };
                    match net::decode_dto(&bytes, &mut resolve, 0, 384) {
                        Ok(dto) => {
                            sources += 1;
                            if dto.source.is_empty {
                                empty += 1;
                            }
                            points += dto.source.columns.iter().map(|c| c.len()).sum::<usize>();
                            println!(
                                "data: {:?} level {} ({} bytes, compression {}, requested {}, sync {sync})",
                                (pos::x(dto.source.pos), pos::z(dto.source.pos)),
                                pos::detail_level(dto.source.pos),
                                bytes.len(),
                                dto.compression,
                                requested.is_some()
                            );
                        }
                        Err(e) => {
                            eprintln!("undecodable data source: {e}");
                            failed += 1;
                        }
                    }
                }
                net::Event::UpToDate(_) => up_to_date += 1,
                net::Event::Gone(s) => {
                    gone += 1;
                    println!("gone: level {} {:?}", pos::detail_level(s), (pos::x(s), pos::z(s)));
                }
                net::Event::LevelInit => println!("level init"),
                net::Event::Closed(reason) => println!("closed: {reason}"),
            }
        }
        let outgoing = client.take_outgoing();
        if !outgoing.is_empty() && !registered {
            registered = true;
            send(&session, "minecraft:register", net::CHANNEL.as_bytes().to_vec());
        }
        for data in outgoing {
            send(&session, net::CHANNEL, data);
        }
        let stats = client.stats();
        if asked && stats.waiting == 0 && stats.in_flight == 0 && sources + gone > 0 {
            if synced.is_some() {
                break;
            }
            let at = distant_horizons::unix_ms();
            for &s in &wanted {
                client.want(s, Some(at), now);
            }
            println!("phase 2: asked again for {} sections as of now", wanted.len());
            synced = Some(wanted.clone());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let stats = client.stats();
    println!(
        "configured {}, generates {}, closed {:?}, last failure {:?}",
        client.configured(),
        client.generates(),
        client.closed(),
        client.last_failure
    );
    println!(
        "{sources} sources ({empty} empty, {points} points), {up_to_date} up to date, {gone} gone, \
         {failed} failed; {} received, {} KiB",
        stats.received,
        stats.bytes / 1024
    );
    let mut common: Vec<_> = names.iter().collect();
    common.sort_by(|a, b| b.1.cmp(a.1));
    println!("{} distinct biome/block entries; most used:", names.len());
    for (name, n) in common.iter().take(12) {
        println!("  {n:5}  {name}");
    }
    for (name, _) in &names {
        let block = name.split(" | ").nth(1).unwrap_or("");
        if block != "AIR" && net::parse_block(block).is_none() {
            println!("  unparsed block: {block}");
            failed += 1;
        }
    }
    session.disconnect();
    let synced_ok = synced.is_some() && up_to_date >= wanted.len() as i32 - 1;
    println!("sync on load: {up_to_date} of {} up to date", wanted.len());
    let ok = client.configured() && level_named_at.is_some() && sources > empty && failed == 0 && synced_ok;
    println!("{}", if ok { "INTEROP OK" } else { "INTEROP FAILED" });
    std::process::exit(if ok { 0 } else { 1 });
}
