use anyhow::{bail, Result};
use peerglide_core::{EdgeGate, PendingReleases, Receiver};
use peerglide_platform::{self as platform, now_ms, Captured};
use peerglide_protocol::{decode, encode, Edge, Input, Message, Motion};
use peerglide_transport as transport;
use std::time::Duration;
use tokio::sync::mpsc;

struct Recovery {
    receiver: Receiver,
    native: bool,
    pending_releases: PendingReleases,
}
impl Recovery {
    fn release(&mut self) -> Result<()> {
        self.pending_releases.extend(self.receiver.release());
        let native = self.native;
        self.pending_releases.flush(|event| {
            if native { platform::inject(event) } else { Ok(()) }
        }).map_err(|remaining| anyhow::anyhow!("native input cleanup failed: {remaining} releases remain pending; session stopped"))
    }
    fn input(&mut self, event: Input) -> Result<()> {
        if self.native {
            platform::inject(event)?;
        }
        Ok(())
    }
    fn pointer(&self, x: f64, y: f64) -> Result<()> {
        if self.native {
            platform::pointer(x, y)?;
        }
        Ok(())
    }
}
impl Drop for Recovery {
    fn drop(&mut self) {
        if self.release().is_err() {
            eprintln!("Native input cleanup remains incomplete; check local input state.");
        }
    }
}
async fn send(tx: &mut quinn::SendStream, m: &Message) -> Result<()> {
    tokio::time::timeout(Duration::from_millis(250), transport::send(tx, m)).await??;
    Ok(())
}
fn normal_close(c: &quinn::Connection) -> bool {
    matches!(c.close_reason(),Some(quinn::ConnectionError::ApplicationClosed(closed)) if closed.error_code==0u8.into())
}
pub async fn run(
    c: quinn::Connection,
    mut tx: quinn::SendStream,
    mut rx: quinn::RecvStream,
    allow: bool,
    native: bool,
    edge: Edge,
) -> Result<()> {
    let (itx, mut input) = mpsc::channel(128);
    let capture = if native {
        Some(platform::capture(itx)?)
    } else {
        None
    };
    let (mtx, mut messages) = mpsc::channel(64);
    let reader = tokio::spawn(async move {
        loop {
            let m = transport::receive(&mut rx).await;
            let failed = m.is_err();
            if mtx.send(m).await.is_err() || failed {
                break;
            }
        }
    });
    struct Abort(tokio::task::JoinHandle<()>);
    impl Drop for Abort {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let _reader = Abort(reader);
    let mut recovery = Recovery {
        receiver: Receiver::default(),
        native,
        pending_releases: PendingReleases::default(),
    };
    let mut pending: Option<(u64, u64)> = None;
    let mut outgoing: Option<u64> = None;
    let mut next_lease = 0u64;
    let mut sequence = 0u64;
    let mut position = (0.5, 0.5);
    let mut local_return = (0.5, 0.5);
    let mut size = (1920.0, 1080.0);
    let mut entry = edge.opposite();
    let mut last_pong = 0u64;
    let mut edge_gate = EdgeGate::default();
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    loop {
        tokio::select! {
            _=tokio::signal::ctrl_c()=>return Ok(()),
            _=c.closed()=>return Ok(()),
            _=tick.tick()=>{
                let now=now_ms();
                if recovery.receiver.expired(now){recovery.release()?;}
                if pending.is_some_and(|(_,start)|now.saturating_sub(start)>1000){pending=None;}
                if let Some(lease)=outgoing {
                    if now.saturating_sub(last_pong)>=1000 {
                        if let Some(cap)=&capture{cap.restore();}
                        outgoing=None; send(&mut tx,&Message::Release{lease}).await?;
                    }else{send(&mut tx,&Message::Ping{lease}).await?;}
                }
                // A full native event queue disables capture in the callback. Reconcile protocol immediately.
                if let (Some(cap),Some(lease))=(&capture,outgoing){if !cap.gate.active(){outgoing=None;send(&mut tx,&Message::Release{lease}).await?;}}
            },
            m=messages.recv()=>{
                let Some(m)=m else{return Ok(());};
                let m=match m {Ok(m)=>m,Err(_) if normal_close(&c)=>return Ok(()),Err(e)=>return Err(e)};
                let now=now_ms();
                match m {
                    Message::Acquire{lease,edge:remote_edge,along}=>{
                        let granted=along.is_finite() && (0.0..=1.0).contains(&along) && pending.is_none() && outgoing.is_none() && capture.as_ref().is_none_or(|cap|cap.gate.idle()) && recovery.receiver.acquire(lease,now,allow);
                        if granted {
                            entry=remote_edge.opposite();position=entry_position(entry,along);
                            recovery.pointer(position.0,position.1)?;
                            send(&mut tx,&Message::Granted{lease}).await?;
                        }else{send(&mut tx,&Message::Denied{lease}).await?;}
                    },
                    Message::Granted{lease}=>{
                        let activated=if pending.is_some_and(|(id,_)|id==lease) {if let Some(cap)=&capture{cap.begin(local_return.0,local_return.1)?}else{false}}else{false};
                        if activated{pending=None;outgoing=Some(lease);sequence=0;last_pong=now;if let Some(cap)=&capture{cap.gate.refresh();}}
                        else{send(&mut tx,&Message::Release{lease}).await?;}
                    },
                    Message::Denied{lease}=>{if pending.is_some_and(|(id,_)|id==lease){pending=None;}},
                    Message::Input{lease,event,cursor}=>{
                        if event.valid() && cursor.lease==lease && recovery.receiver.anchor(cursor,now) && recovery.receiver.input(lease,event,now) {
                            recovery.pointer(cursor.x,cursor.y)?;recovery.input(event)?;
                        }
                    },
                    Message::Release{lease}=>{
                        if recovery.receiver.active()==Some(lease){recovery.release()?;}
                        if outgoing==Some(lease){outgoing=None;if let Some(cap)=&capture{cap.restore();}}
                    },
                    Message::Ping{lease}=>{if lease==0 || recovery.receiver.heartbeat(lease,now){send(&mut tx,&Message::Pong{lease}).await?;}},
                    Message::Pong{lease}=>{if outgoing==Some(lease){last_pong=now;if let Some(cap)=&capture{cap.gate.refresh();}}},
                    Message::Hello{..}=>bail!("unexpected repeated handshake"),
                }
            },
            d=c.read_datagram()=>{
                let d=match d {Ok(d)=>d,Err(_) if normal_close(&c)=>return Ok(()),Err(e)=>return Err(e.into())};
                if let Ok(m)=decode::<Motion>(&d) {if recovery.receiver.motion(m,now_ms()) {
                    recovery.pointer(m.x,m.y)?;
                    let dist=distance(entry,m.x,m.y);
                    if dist<=0.001 {
                        recovery.release()?;send(&mut tx,&Message::Release{lease:m.lease}).await?;
                    }
                }}
            },
            e=input.recv(),if native=>{
                let Some(e)=e else{bail!("native capture channel closed");};
                if matches!(e,Captured::Position{..}|Captured::Input(_)|Captured::Toggle) {
                    if let Some(lease)=recovery.receiver.active(){recovery.release()?;send(&mut tx,&Message::Release{lease}).await?;}
                }
                match e {
                    Captured::Emergency=>{
                        if let Some(cap)=&capture{cap.restore();}
                        pending=None; if let Some(lease)=outgoing.take(){send(&mut tx,&Message::Release{lease}).await?;}
                        if let Some(lease)=recovery.receiver.active(){recovery.release()?;send(&mut tx,&Message::Release{lease}).await?;}
                    },
                    Captured::Position{x,y,width,height}=>{
                        size=(width.max(1.0),height.max(1.0));
                        let nx=(x/(size.0-1.0).max(1.0)).clamp(0.0,1.0);let ny=(y/(size.1-1.0).max(1.0)).clamp(0.0,1.0);
                        let cross=edge_gate.update(distance(edge,nx,ny)<=0.001,now_ms());
                        if cross && capture.as_ref().is_some_and(|cap|cap.gate.idle()) && outgoing.is_none() && pending.is_none() && recovery.receiver.active().is_none(){
                            next_lease=next_lease.checked_add(1).ok_or_else(||anyhow::anyhow!("lease counter exhausted"))?;
                            let along=match edge{Edge::Left|Edge::Right=>ny,_=>nx};position=entry_position(edge.opposite(),along);local_return=entry_position(edge,along);
                            pending=Some((next_lease,now_ms()));send(&mut tx,&Message::Acquire{lease:next_lease,edge,along}).await?;
                        }
                    },
                    Captured::Toggle=>{
                        if let Some(lease)=outgoing.take(){if let Some(cap)=&capture{cap.restore();}send(&mut tx,&Message::Release{lease}).await?;}
                        else if pending.is_none() && recovery.receiver.active().is_none(){next_lease+=1;position=entry_position(edge.opposite(),0.5);local_return=entry_position(edge,0.5);pending=Some((next_lease,now_ms()));send(&mut tx,&Message::Acquire{lease:next_lease,edge,along:0.5}).await?;}
                    },
                    Captured::Motion{dx,dy}=>{if let Some(lease)=outgoing {
                        position.0=(position.0+dx/size.0).clamp(0.0,1.0);position.1=(position.1+dy/size.1).clamp(0.0,1.0);
                        sequence+=1;let m=Motion{lease,sequence,x:position.0,y:position.1};
                        // Datagram queue pressure may discard motion; the next absolute position repairs it.
                        let _=c.send_datagram(encode(&m)?.into());
                    }},
                    Captured::Input(event)=>{if let Some(lease)=outgoing{
                        sequence+=1;let cursor=Motion{lease,sequence,x:position.0,y:position.1};
                        send(&mut tx,&Message::Input{lease,event,cursor}).await?;
                    }},
                }
            }
        }
    }
}
fn distance(edge: Edge, x: f64, y: f64) -> f64 {
    match edge {
        Edge::Left => x,
        Edge::Right => 1.0 - x,
        Edge::Top => y,
        Edge::Bottom => 1.0 - y,
    }
}
fn entry_position(edge: Edge, along: f64) -> (f64, f64) {
    match edge {
        Edge::Left => (0.002, along),
        Edge::Right => (0.998, along),
        Edge::Top => (along, 0.002),
        Edge::Bottom => (along, 0.998),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use transport::Identity;

    async fn exercise(allow: bool) -> Result<()> {
        let a = Identity::generate()?;
        let b = Identity::generate()?;
        let ea = transport::endpoint("127.0.0.1:0".parse()?, &a, &b.certificate)?;
        let eb = transport::endpoint("127.0.0.1:0".parse()?, &b, &a.certificate)?;
        let server = async {
            let c = eb.accept().await.unwrap().await?;
            let (tx, rx) = c.accept_bi().await?;
            run(c, tx, rx, allow, false, Edge::Right).await
        };
        let client = async {
            let c = ea.connect(eb.local_addr()?, "peerglide.local")?.await?;
            let (mut tx, mut rx) = c.open_bi().await?;
            send(&mut tx, &Message::Ping { lease: 0 }).await?;
            assert!(matches!(
                transport::receive(&mut rx).await?,
                Message::Pong { lease: 0 }
            ));
            send(
                &mut tx,
                &Message::Acquire {
                    lease: 1,
                    edge: Edge::Right,
                    along: 0.5,
                },
            )
            .await?;
            let response = transport::receive(&mut rx).await?;
            if allow {
                assert!(matches!(response, Message::Granted { lease: 1 }));
                send(
                    &mut tx,
                    &Message::Input {
                        lease: 1,
                        cursor: Motion {
                            lease: 1,
                            sequence: 1,
                            x: 0.5,
                            y: 0.5,
                        },
                        event: Input::Key {
                            usage: 0xe0,
                            down: true,
                        },
                    },
                )
                .await?;
                send(&mut tx, &Message::Ping { lease: 1 }).await?;
                assert!(matches!(
                    transport::receive(&mut rx).await?,
                    Message::Pong { lease: 1 }
                ));
                send(
                    &mut tx,
                    &Message::Acquire {
                        lease: 2,
                        edge: Edge::Right,
                        along: 0.5,
                    },
                )
                .await?;
                assert!(matches!(
                    transport::receive(&mut rx).await?,
                    Message::Denied { lease: 2 }
                ));
                tokio::time::sleep(Duration::from_millis(1800)).await;
                send(
                    &mut tx,
                    &Message::Acquire {
                        lease: 3,
                        edge: Edge::Right,
                        along: 0.5,
                    },
                )
                .await?;
                assert!(matches!(
                    transport::receive(&mut rx).await?,
                    Message::Granted { lease: 3 }
                ));
                c.send_datagram(
                    encode(&Motion {
                        lease: 3,
                        sequence: 1,
                        x: 0.0,
                        y: 0.5,
                    })?
                    .into(),
                )?;
                assert!(matches!(
                    transport::receive(&mut rx).await?,
                    Message::Release { lease: 3 }
                ));
                send(
                    &mut tx,
                    &Message::Acquire {
                        lease: 1,
                        edge: Edge::Left,
                        along: 0.5,
                    },
                )
                .await?;
                assert!(matches!(
                    transport::receive(&mut rx).await?,
                    Message::Denied { lease: 1 }
                ));
            } else {
                assert!(matches!(response, Message::Denied { lease: 1 }));
            }
            c.close(0u8.into(), b"done");
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<_, anyhow::Error>(())
        };
        tokio::time::timeout(Duration::from_secs(8), async {
            tokio::try_join!(server, client)
        })
        .await??;
        Ok(())
    }
    #[tokio::test]
    async fn default_denies_input() -> Result<()> {
        exercise(false).await
    }
    #[tokio::test]
    async fn live_session_grants_conflicts_expires_and_rejects_replay() -> Result<()> {
        exercise(true).await
    }
}
