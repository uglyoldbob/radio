//! PID code for hvac controls

/// The mode for a pid controller
#[derive(PartialEq)]
pub enum PidMode {
    /// More duty cycle generally increases the measured value
    Increasing,
    /// More duty cycle generally decreases the measured value
    Decreasing,
}

/// The main pid structure
pub struct Pid {
    /// The calculated duty cycle output
    dc_out: f32,
    /// The input value
    input: f32,
    /// The setpoint of the value
    setpoint: f32,
    /// The proportional constant
    kp: f32,
    /// The integral constant
    ki: f32,
    /// The derivative constant
    kd: f32,
    /// Integral storage
    is: f32,
    /// The time for the last update
    last_update: std::time::Instant,
    /// The mode of pid operation
    mode: PidMode,
}

impl Pid {
    /// Construct a new self
    pub fn new(mode: PidMode, kp: f32, ki: f32, kd: f32) -> Self {
        Self {
            dc_out: 0.0,
            input: 0.0,
            setpoint: 0.0,
            kp,
            ki,
            kd,
            is: 0.0,
            last_update: std::time::Instant::now(),
            mode,
        }
    }

    /// Get the duty cycle
    pub fn duty_cycle(&self) -> f32 {
        self.dc_out
    }

    /// Update the pid constants
    pub fn set_pid_constants(&mut self, kp: f32, ki: f32, kd: f32) {
        self.kp = kp;
        self.ki = ki;
        self.kd = kd;
    }

    /// Reset the pid controller
    pub fn reset(&mut self) {
        self.is = 0.0;
        self.dc_out = 0.0;
    }

    /// Set the setpoint for the pid algorithm
    pub fn set_setpoint(&mut self, val: f32) {
        self.setpoint = val;
    }

    /// Change the mode of the pid
    pub fn change_mode(&mut self, mode: PidMode) {
        if self.mode != mode {
            self.mode = mode;
            self.reset();
        }
    }

    /// Run the duty cycle calculation with the new measurement
    pub fn run_calc(&mut self, val: f32) {
        let error = match self.mode {
            PidMode::Decreasing => val - self.setpoint,
            PidMode::Increasing => self.setpoint - val,
        };
        let now = std::time::Instant::now();
        let deltat = now - self.last_update;
        let deltat = deltat.as_nanos() as f32 / 1000000000.0;
        self.last_update = now;
        let derivative = match self.mode {
            PidMode::Decreasing => (val - self.input) / deltat,
            PidMode::Increasing => (self.input - val) / deltat,
        };
        self.input = val;
        let derivative = derivative * self.kd;
        let proportional = error * self.kp;
        self.is += self.ki * error;
        if self.is > 1.0 {
            self.is = 1.0;
        }
        if self.is < -1.0 {
            self.is = -1.0;
        }
        let proportional = if proportional < -1.0 {
            -1.0
        } else if proportional > 1.0 {
            1.0
        } else {
            proportional
        };
        let derivative = if derivative < -1.0 {
            -1.0
        } else if derivative > 1.0 {
            1.0
        } else {
            derivative
        };
        self.dc_out = proportional + self.is + derivative;
        if self.dc_out > 1.0 {
            self.dc_out = 1.0;
        }
        if self.dc_out < -1.0 {
            self.dc_out = -1.0;
        }
    }
}
