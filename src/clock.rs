use chrono::{DateTime, Local};

pub trait Clock {
    fn now(&self) -> DateTime<Local>;
}

#[derive(Clone, Copy, Debug)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Local> {
        Local::now()
    }
}

#[cfg(test)]
pub mod tests {
    use chrono::{DateTime, Local, TimeZone};

    use super::Clock;

    #[derive(Clone, Copy, Debug)]
    pub struct FixedClock {
        pub hour: u32,
    }

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Local> {
            Local
                .with_ymd_and_hms(2026, 5, 19, self.hour, 15, 0)
                .unwrap()
        }
    }
}
