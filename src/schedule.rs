use anyhow::{bail, Result};

use crate::config::{validate_slots, ScheduleSlot};

pub fn select_slot(slots: &[ScheduleSlot], current_hour: u32) -> Result<&ScheduleSlot> {
    if current_hour > 23 {
        bail!("current hour {current_hour} is out of range");
    }
    validate_slots(slots)?;

    let mut sorted: Vec<&ScheduleSlot> = slots.iter().collect();
    sorted.sort_by_key(|slot| slot.hour);

    Ok(sorted
        .iter()
        .rev()
        .copied()
        .find(|slot| u32::from(slot.hour) <= current_hour)
        .unwrap_or_else(|| *sorted.last().expect("validate_slots rejects empty slots")))
}

#[cfg(test)]
mod tests {
    use crate::config::ScheduleSlot;

    use super::select_slot;

    fn slots() -> Vec<ScheduleSlot> {
        vec![
            ScheduleSlot {
                hour: 6,
                profile: "morning".to_string(),
            },
            ScheduleSlot {
                hour: 10,
                profile: "day".to_string(),
            },
            ScheduleSlot {
                hour: 20,
                profile: "night".to_string(),
            },
        ]
    }

    #[test]
    fn selects_latest_slot_before_current_hour() {
        assert_eq!(select_slot(&slots(), 18).unwrap().profile, "day");
        assert_eq!(select_slot(&slots(), 20).unwrap().profile, "night");
    }

    #[test]
    fn wraps_before_first_hour() {
        assert_eq!(select_slot(&slots(), 2).unwrap().profile, "night");
    }
}
