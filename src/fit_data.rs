use chrono::{DateTime, Local};
use fitparser::{de, profile::MesgNum, FitDataField, Value};

#[derive(Default, Clone)]
pub struct ActivityData {
    pub filename: String,
    pub sport: Option<String>,
    pub records: Vec<Record>,
    pub laps: Vec<Lap>,
    pub total_distance_m: f64,
    pub total_time_s: f64,
    pub total_timer_time: f64,
    pub total_ascent_m: u32,
    pub avg_hr_bpm: u8,
    pub max_hr_bpm: u8,
    pub avg_speed_ms: f64,
    pub avg_power_w: u16,
    pub max_power_w: u16,
    /// Metadata messages (everything except Record / Hrv — too many).
    pub info_messages: Vec<RawMessage>,
    pub highlights: Highlights,
}

#[derive(Clone, Default)]
pub struct Record {
    pub timestamp: Option<DateTime<Local>>,
    pub elapsed_s: f64,
    pub distance_m: f64,
    pub heart_rate: Option<u8>,
    pub speed_ms: Option<f64>,
    pub altitude_m: Option<f64>,
    pub power_w: Option<u16>,
    pub cadence: Option<u8>,
    pub respiration_rate: Option<f64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

#[derive(Clone, Default)]
pub struct Lap {
    pub index: usize,
    pub distance_m: f64,
    pub time_s: f64,
    pub avg_hr: Option<u8>,
    pub max_hr: Option<u8>,
    pub avg_speed_ms: Option<f64>,
    pub avg_power_w: Option<u16>,
    pub ascent_m: Option<u16>,
}

/// A single decoded FIT message (excluding Record/Hrv) for the Info tab.
#[derive(Clone, Default)]
pub struct RawMessage {
    pub kind: String,
    pub fields: Vec<RawField>,
}

#[derive(Clone, Default)]
pub struct RawField {
    pub name: String,
    pub value: String,
    pub units: String,
}

#[derive(Clone, Default)]
pub struct Highlights {
    pub vo2_max: Option<f64>,
    pub aerobic_te: Option<f64>,
    pub anaerobic_te: Option<f64>,
    pub intensity_factor: Option<f64>,
    pub training_stress_score: Option<f64>,
    pub total_calories: Option<u32>,
    pub total_cycles: Option<u32>, // steps / strokes depending on sport
    pub device_name: Option<String>,
    pub software_version: Option<String>,
}

fn field_f64(fields: &[FitDataField], name: &str) -> Option<f64> {
    fields
        .iter()
        .find(|f| f.name() == name)
        .and_then(|f| match f.value() {
            Value::Float64(v) => Some(*v),
            Value::Float32(v) => Some(*v as f64),
            Value::SInt8(v) => Some(*v as f64),
            Value::UInt8(v) => Some(*v as f64),
            Value::SInt16(v) => Some(*v as f64),
            Value::UInt16(v) => Some(*v as f64),
            Value::SInt32(v) => {
                if name.starts_with("position_") {
                    Some(*v as f64 * 180.0 / 2_147_483_648.0)
                } else {
                    Some(*v as f64)
                }
            }
            Value::UInt32(v) => Some(*v as f64),
            _ => None,
        })
}

fn field_u8(fields: &[FitDataField], name: &str) -> Option<u8> {
    fields
        .iter()
        .find(|f| f.name() == name)
        .and_then(|f| match f.value() {
            Value::UInt8(v) => Some(*v),
            Value::Byte(v) => Some(*v),
            Value::Enum(v) => Some(*v),
            _ => None,
        })
}

fn field_u16(fields: &[FitDataField], name: &str) -> Option<u16> {
    fields
        .iter()
        .find(|f| f.name() == name)
        .and_then(|f| match f.value() {
            Value::UInt16(v) => Some(*v),
            Value::UInt8(v) => Some(*v as u16),
            _ => None,
        })
}

fn field_u32(fields: &[FitDataField], name: &str) -> Option<u32> {
    fields
        .iter()
        .find(|f| f.name() == name)
        .and_then(|f| match f.value() {
            Value::UInt32(v) => Some(*v),
            Value::UInt16(v) => Some(*v as u32),
            Value::UInt8(v) => Some(*v as u32),
            _ => None,
        })
}

fn field_str(fields: &[FitDataField], name: &str) -> Option<String> {
    fields
        .iter()
        .find(|f| f.name() == name)
        .map(|f| format!("{}", f.value()))
}

fn field_timestamp(fields: &[FitDataField]) -> Option<DateTime<Local>> {
    fields
        .iter()
        .find(|f| f.name() == "timestamp")
        .and_then(|f| match f.value() {
            Value::Timestamp(dt) => Some(*dt),
            _ => None,
        })
}

pub fn parse_fit(bytes: &[u8], filename: String) -> Result<ActivityData, String> {
    let msgs = de::from_bytes(bytes).map_err(|e| e.to_string())?;

    let mut activity = ActivityData {
        filename,
        ..Default::default()
    };
    let mut start_time: Option<DateTime<Local>> = None;
    let mut lap_index = 0usize;

    for msg in &msgs {
        let fields = msg.fields();

        match msg.kind() {
            MesgNum::Record => {
                let ts = field_timestamp(fields);
                if start_time.is_none() {
                    start_time = ts;
                }
                let elapsed_s = match (ts, start_time) {
                    (Some(t), Some(s)) => (t - s).num_milliseconds() as f64 / 1000.0,
                    _ => activity.records.len() as f64,
                };
                activity.records.push(Record {
                    timestamp: ts,
                    elapsed_s,
                    distance_m: field_f64(fields, "distance").unwrap_or(0.0),
                    heart_rate: field_u8(fields, "heart_rate"),
                    speed_ms: field_f64(fields, "speed").or_else(|| field_f64(fields, "enhanced_speed")),
                    altitude_m: field_f64(fields, "altitude").or_else(|| field_f64(fields, "enhanced_altitude")),
                    power_w: field_u16(fields, "power"),
                    cadence: field_u8(fields, "cadence"),
                    respiration_rate: field_f64(fields, "respiration_rate").or_else(|| field_f64(fields, "enhanced_respiration_rate")),
                    lat: field_f64(fields, "position_lat"),
                    lon: field_f64(fields, "position_long"),
                });
                continue; // skip raw-message capture for Record
            }
            MesgNum::Lap => {
                activity.laps.push(Lap {
                    index: lap_index,
                    distance_m: field_f64(fields, "total_distance").unwrap_or(0.0),
                    time_s: field_f64(fields, "total_elapsed_time").unwrap_or(0.0),
                    avg_hr: field_u8(fields, "avg_heart_rate"),
                    max_hr: field_u8(fields, "max_heart_rate"),
                    avg_speed_ms: field_f64(fields, "avg_speed")
                        .or_else(|| field_f64(fields, "enhanced_avg_speed")),
                    avg_power_w: field_u16(fields, "avg_power"),
                    ascent_m: field_u16(fields, "total_ascent"),
                });
                lap_index += 1;
                // Fall through to also capture Lap as a raw message
            }
            MesgNum::Session => {
                activity.sport = field_str(fields, "sport");
                activity.total_distance_m = field_f64(fields, "total_distance").unwrap_or(0.0);
                activity.total_time_s = field_f64(fields, "total_elapsed_time").unwrap_or(0.0);
                activity.total_timer_time = field_f64(fields, "total_timer_time").unwrap_or(0.0);
                activity.avg_hr_bpm = field_u8(fields, "avg_heart_rate").unwrap_or(0);
                activity.max_hr_bpm = field_u8(fields, "max_heart_rate").unwrap_or(0);
                activity.avg_speed_ms = field_f64(fields, "avg_speed").or_else(|| field_f64(fields, "enhanced_avg_speed")).unwrap_or(0.0);
                activity.avg_power_w = field_u16(fields, "avg_power").unwrap_or(0);
                activity.max_power_w = field_u16(fields, "max_power").unwrap_or(0);
                activity.total_ascent_m = field_u32(fields, "total_ascent").unwrap_or(0);
                // Highlights from Session
                activity.highlights.aerobic_te = field_f64(fields, "total_training_effect");
                activity.highlights.anaerobic_te = field_f64(fields, "total_anaerobic_training_effect");
                activity.highlights.intensity_factor = field_f64(fields, "intensity_factor");
                activity.highlights.training_stress_score = field_f64(fields, "training_stress_score");
                activity.highlights.total_calories = field_u32(fields, "total_calories");
                activity.highlights.total_cycles = field_u32(fields, "total_cycles");
            }
            MesgNum::DeviceInfo => {
                if activity.highlights.device_name.is_none() {
                    let name = fields
                        .iter()
                        .find(|f| f.name() == "product_name" || f.name() == "garmin_product")
                        .map(|f| format!("{}", f.value()))
                        .filter(|s| !s.is_empty() && s != "0");
                    if name.is_some() {
                        activity.highlights.device_name = name;
                    }
                }
                if activity.highlights.software_version.is_none() {
                    activity.highlights.software_version = fields
                        .iter()
                        .find(|f| f.name() == "software_version")
                        .map(|f| format!("{}", f.value()))
                        .filter(|s| s != "0" && !s.is_empty());
                }
            }
            MesgNum::Value(140) => {
                if let Some(v) = field_f64(fields, "unknown_field_7") {
                    let normalized_value = v * 3.5 / 65536.0;
                    if normalized_value > 0.0 && normalized_value < 100.0 {
                        activity.highlights.vo2_max = Some(normalized_value);
                    }
                }
            }
            // Skip HRV measurements — they fire once per heartbeat and can number thousands
            MesgNum::Hrv => continue,
            _ => {}
        }

        activity.info_messages.push(RawMessage {
            kind: format!("{:?}", msg.kind()),
            fields: fields
                .iter()
                .map(|f| RawField {
                    name: f.name().to_string(),
                    value: format!("{}", f.value()),
                    units: f.units().to_string(),
                })
                .collect(),
        });
    }

    // Fallback scan for VO₂ max. Only runs if the extraction above didn't find it.
    if activity.highlights.vo2_max.is_none() {
        'vo2: for msg in &activity.info_messages {
            for f in &msg.fields {
                if msg.kind == "Value(140)" && f.name == "unknown_field_7" {
                    if let Ok(v) = f.value.parse::<f64>() {
                        let normalized_value = v * 3.5 / 65536.0;
                        if normalized_value > 0.0 && normalized_value < 100.0 {
                            activity.highlights.vo2_max = Some(v * 3.5 / 65536.0);
                            break 'vo2;
                        }
                    }
                }
            }
        }
    }

    // Derive session totals from records when a Session message is absent
    if activity.total_time_s == 0.0 {
        if let Some(last) = activity.records.last() {
            activity.total_time_s = last.elapsed_s;
            activity.total_distance_m = last.distance_m;
        }
    }

    // Compute avg_speed from distance / time when the Session field was absent/zero.
    if activity.avg_speed_ms == 0.0 && activity.total_timer_time > 0.0 && activity.total_distance_m > 0.0 {
        activity.avg_speed_ms = activity.total_distance_m / activity.total_timer_time;
    }

    // Compute per-lap avg_speed from distance / time when the Lap field was absent.
    for lap in &mut activity.laps {
        if lap.avg_speed_ms.is_none() && lap.time_s > 0.0 && lap.distance_m > 0.0 {
            lap.avg_speed_ms = Some(lap.distance_m / lap.time_s);
        }
    }

    Ok(activity)
}