use anyhow::{bail, Result};
use peerglide_core::{EdgeGate, Receiver};
use peerglide_platform::{self as platform, now_ms, Captured};
use peerglide_protocol::{decode, encode, Edge, Input, Message, Motion};
use peerglide_transport as transport;
use std::time::Duration;
use tokio::sync::mpsc;

struct Recovery {
    receiver: Receiver,
    native: bool,
}
impl Recovery {
    fn release(&mut self) {
        for event in self.receiver.release() {
            if self.native {
                let _ = platform::inject(event);
            }
        }
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
        self.release();
    }
}
async fn send(tx: &mut quinn::SendStream, m: &Message) -> Result<()> {
    tokio::time::timeout(Duration::from_millis(250), transport::send(tx, m)).await??;
    Ok(())
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
    };
    let mut pending: Option<(u64, u64)> = None;
    let mut outgoing: Option<u64> = None;
    let mut next_lease = 0u64;
    let mut sequence = 0u64;
    let mut position = (0.5, 0.5);
    let mut size = (1920.0, 1080.0);
    let mut entry = edge.opposite();
    let mut incoming_moved = false;
    let mut last_pong = 0u64;
    let mut edge_gate = EdgeGate::default();
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    loop {
        tokio::select! {
            _=tokio::signal::ctrl_c()=>return Ok(()),
            _=c.closed()=>return Ok(()),
            _=tick.tick()=>{
                let now=now_ms();
                if recovery.receiver.expired(now){recovery.release();}
                if pending.is_some_and(|(_,start)|now.saturating_sub(start)>1000){pending=None;}
                if let Some(lease)=outgoing {
                    if now.saturating_sub(last_pong)>=1000 {
                        if let Some(cap)=&capture{cap.gate.disable();}
                        outgoing=None; send(&mut tx,&Message::Release{lease}).await?;
                    }else{send(&mut tx,&Message::Ping{lease}).await?;}
                }
                // A full native event queue disables capture in the callback. Reconcile protocol immediately.
                if let (Some(cap),Some(lease))=(&capture,outgoing){if !cap.gate.active(){outgoing=None;send(&mut tx,&Message::Release{lease}).await?;}}
            },
            m=messages.recv()=>{
                let Some(m)=m else{return Ok(());};let m=m?;let now=now_ms();
                match m {
                    Message::Acquire{lease,edge:remote_edge,along}=>{
                        let granted=along.is_finite() && (0.0..=1.0).contains(&along) && pending.is_none() && outgoing.is_none() && recovery.receiver.acquire(lease,now,allow);
                        if granted {
                            entry=remote_edge.opposite();position=entry_position(entry,along);incoming_moved=false;
                            recovery.pointer(position.0,position.1)?;
                            send(&mut tx,&Message::Granted{lease}).await?;
                        }else{send(&mut tx,&Message::Denied{lease}).await?;}
                    },
                    Message::Granted{lease}=>{
                        if pending.is_some_and(|(id,_)|id==lease) && capture.as_ref().is_some_and(|cap|cap.gate.idle()){pending=None;outgoing=Some(lease);sequence=0;last_pong=now;if let Some(cap)=&capture{cap.gate.refresh();}}
                        else{send(&mut tx,&Message::Release{lease}).await?;}
                    },
                    Message::Denied{lease}=>{if pending.is_some_and(|(id,_)|id==lease){pending=None;}},
                    Message::Input{lease,event}=>{if recovery.receiver.input(lease,event,now){recovery.input(event)?;}},
                    Message::Release{lease}=>{
                        if recovery.receiver.active()==Some(lease){recovery.release();}
                        if outgoing==Some(lease){outgoing=None;if let Some(cap)=&capture{cap.gate.disable();}}
                    },
                    Message::Ping{lease}=>{if lease==0 || recovery.receiver.heartbeat(lease,now){send(&mut tx,&Message::Pong{lease}).await?;}},
                    Message::Pong{lease}=>{if outgoing==Some(lease){last_pong=now;if let Some(cap)=&capture{cap.gate.refresh();}}},
                    Message::Hello{..}=>bail!("unexpected repeated handshake"),
                }
            },
            d=c.read_datagram()=>{
                let d=d?;
                if let Ok(m)=decode::<Motion>(&d) {if recovery.receiver.motion(m,now_ms()) {
                    recovery.pointer(m.x,m.y)?;
                    let dist=distance(entry,m.x,m.y);
                    if dist>0.02 {incoming_moved=true;}
                    if incoming_moved && dist<=0.001 {
                        recovery.release();send(&mut tx,&Message::Release{lease:m.lease}).await?;
                    }
                }}
            },
            e=input.recv(),if native=>{
                let Some(e)=e else{bail!("native capture channel closed");};
                if matches!(e,Captured::Position{..}|Captured::Input(_)|Captured::Toggle) {
                    if let Some(lease)=recovery.receiver.active(){recovery.release();send(&mut tx,&Message::Release{lease}).await?;}
                }
                match e {
                    Captured::Emergency=>{
                        if let Some(cap)=&capture{cap.gate.disable();}
                        pending=None; if let Some(lease)=outgoing.take(){send(&mut tx,&Message::Release{lease}).await?;}
                        if let Some(lease)=recovery.receiver.active(){recovery.release();send(&mut tx,&Message::Release{lease}).await?;}
                    },
                    Captured::Position{x,y,width,height}=>{
                        size=(width.max(1.0),height.max(1.0));
                        let nx=(x/(size.0-1.0).max(1.0)).clamp(0.0,1.0);let ny=(y/(size.1-1.0).max(1.0)).clamp(0.0,1.0);
                        let cross=edge_gate.update(distance(edge,nx,ny)<=0.001,now_ms());
                        if cross && capture.as_ref().is_some_and(|cap|cap.gate.idle()) && outgoing.is_none() && pending.is_none() && recovery.receiver.active().is_none(){
                            next_lease=next_lease.checked_add(1).ok_or_else(||anyhow::anyhow!("lease counter exhausted"))?;
                            let along=match edge{Edge::Left|Edge::Right=>ny,_=>nx};position=entry_position(edge.opposite(),along);
                            pending=Some((next_lease,now_ms()));send(&mut tx,&Message::Acquire{lease:next_lease,edge,along}).await?;
                        }
                    },
                    Captured::Toggle=>{
                        if let Some(lease)=outgoing.take(){if let Some(cap)=&capture{cap.gate.disable();}send(&mut tx,&Message::Release{lease}).await?;}
                        else if pending.is_none() && recovery.receiver.active().is_none(){next_lease+=1;position=entry_position(edge.opposite(),0.5);pending=Some((next_lease,now_ms()));send(&mut tx,&Message::Acquire{lease:next_lease,edge,along:0.5}).await?;}
                    },
                    Captured::Motion{dx,dy}=>{if let Some(lease)=outgoing {
                        position.0=(position.0+dx/size.0).clamp(0.0,1.0);position.1=(position.1+dy/size.1).clamp(0.0,1.0);
                        sequence+=1;let m=Motion{lease,sequence,x:position.0,y:position.1};
                        // Datagram queue pressure may discard motion; the next absolute position repairs it.
                        let _=c.send_datagram(encode(&m)?.into());
                    }},
                    Captured::Input(event)=>{if let Some(lease)=outgoing{send(&mut tx,&Message::Input{lease,event}).await?;}},
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
