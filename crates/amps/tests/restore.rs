//! `Amp::restore` write order, with a recording amp.

use std::any::Any;
use std::sync::Mutex;

use async_trait::async_trait;
use labgruppen::*;
use tokio::sync::broadcast;

struct Recorder {
    info: AmpInfo,
    log: Mutex<Vec<String>>,
}

#[async_trait]
impl Amp for Recorder {
    fn info(&self) -> &AmpInfo {
        &self.info
    }
    fn params(&self) -> Vec<Param> {
        vec![]
    }
    fn spec(&self, p: &Param) -> Option<ParamSpec> {
        Some(match p {
            Param::Mute(_)
            | Param::Eq { field: EqField::Enabled, .. }
            | Param::Xover { field: XoverField::Enabled, .. } => ParamSpec::new(Kind::Bool),
            Param::Eq { field: EqField::Type, .. } => ParamSpec::new(Kind::Enum(EQ_TYPES)),
            Param::Xover { field: XoverField::Type, .. } => ParamSpec::new(Kind::Enum(XOVER_TYPES)),
            Param::Limiter { field: LimiterField::Power, .. } => ParamSpec::new(Kind::Float).read_only(),
            _ => ParamSpec::new(Kind::Float),
        })
    }
    async fn get(&self, _: &Param) -> Result<Value> {
        Err(Error::Unsupported("get".into()))
    }
    async fn set(&self, p: &Param, v: Value) -> Result<()> {
        self.log.lock().unwrap().push(format!("{p}={v}"));
        Ok(())
    }
    async fn status(&self) -> Result<Status> {
        Ok(Status::default())
    }
    fn preset_slots(&self) -> std::ops::RangeInclusive<u32> {
        1..=50
    }
    async fn presets(&self) -> Result<Vec<PresetInfo>> {
        Ok(vec![])
    }
    async fn recall_preset(&self, _: u32) -> Result<()> {
        Ok(())
    }
    async fn store_preset(&self, _: u32, _: &str) -> Result<()> {
        Ok(())
    }
    fn meters(&self) -> Meters {
        Meters::default()
    }
    fn subscribe(&self) -> broadcast::Receiver<AmpEvent> {
        broadcast::channel(1).1
    }
    async fn close(&self) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[tokio::test]
async fn restore_mutes_first_unmutes_last_and_enables_after_settings() {
    let amp = Recorder {
        info: AmpInfo { driver: "test", host: String::new(), model: String::new() },
        log: Mutex::new(vec![]),
    };
    let mut snap = Snapshot::new("test", "t");
    for (k, v) in [
        ("out1.mute", Value::Bool(false)),
        ("out2.mute", Value::Bool(true)),
        ("out1.xover.lp.enabled", Value::Bool(false)),
        ("out1.xover.lp.type", Value::Text("bessel_12".into())),
        ("out1.gain", Value::Float(-3.0)),
        ("out1.eq.0.enabled", Value::Bool(true)),
        ("out1.eq.0.type", Value::Text("bell".into())),
        ("out1.limiter.power", Value::Float(595.0)),
        ("out1.bogus", Value::Int(1)),
    ] {
        snap.params.insert(k.into(), v);
    }
    let report = amp.restore(&snap, false).await.unwrap();
    assert_eq!(report.applied, 7);
    assert_eq!(report.skipped, ["out1.bogus", "out1.limiter.power"]);
    let log = amp.log.lock().unwrap().clone();
    assert_eq!(log.first().unwrap(), "out2.mute=on");
    assert_eq!(log.last().unwrap(), "out1.mute=off");
    let pos = |s: &str| log.iter().position(|l| l.starts_with(s)).unwrap();
    assert!(pos("out1.xover.lp.type") < pos("out1.xover.lp.enabled"));
    assert!(pos("out1.eq.0.type") < pos("out1.eq.0.enabled"));
    assert!(pos("out1.gain") < pos("out1.xover.lp.enabled"));
}

#[tokio::test]
async fn dry_run_writes_nothing() {
    let amp = Recorder {
        info: AmpInfo { driver: "test", host: String::new(), model: String::new() },
        log: Mutex::new(vec![]),
    };
    let mut snap = Snapshot::new("test", "t");
    snap.params.insert("out1.gain".into(), Value::Float(1.0));
    snap.params.insert("out1.gain".into(), Value::Float(200.0));
    let report = amp.restore(&snap, true).await.unwrap();
    assert_eq!((report.applied, report.failed.len()), (1, 0));
    assert!(amp.log.lock().unwrap().is_empty());
}
