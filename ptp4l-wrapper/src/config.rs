/*
 *  Copyright (C) 2025 Michael Bachmann
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{fmt, path::PathBuf};

#[derive(Debug, Clone)]
pub enum TimeStamping {
    Hardware,
    Software,
    Legacy,
}

impl fmt::Display for TimeStamping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Hardware => "hardware",
            Self::Software => "software",
            Self::Legacy => "legacy",
        })
    }
}

#[derive(Debug, Clone)]
pub enum NetworkTransport {
    UdpV4,
    UdpV6,
    L2,
}

impl fmt::Display for NetworkTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UdpV4 => "UDPv4",
            Self::UdpV6 => "UDPv6",
            Self::L2 => "L2",
        })
    }
}

#[derive(Debug, Clone)]
pub enum DelayMechanism {
    E2E,
    P2P,
}

impl fmt::Display for DelayMechanism {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::E2E => "E2E",
            Self::P2P => "P2P",
        })
    }
}

#[derive(Debug, Clone)]
pub enum ClockServo {
    Pi,
    LinReg,
}

impl fmt::Display for ClockServo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pi => "pi",
            Self::LinReg => "linreg",
        })
    }
}

#[derive(Debug, Clone)]
pub enum TsProcMode {
    Filter,
    Raw,
}

impl fmt::Display for TsProcMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Filter => "filter",
            Self::Raw => "raw",
        })
    }
}

#[derive(Debug, Clone)]
pub enum DelayFilter {
    MovingMedian,
    MovingAverage,
}

impl fmt::Display for DelayFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MovingMedian => "moving_median",
            Self::MovingAverage => "moving_average",
        })
    }
}

/// Global ptp4l settings (the `[global]` section).
///
/// Interface names are passed separately to [`start_ptpt4l`].
/// All fields are optional; only `Some` values are written to the config file.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub global: GlobalConfig,
}

/// Renders a complete ptp4l config file accepted by `ptp4l -f <file>`.
///
/// Interface sections are omitted; use [`Config::display_with_interfaces`]
/// to include them.
impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_config(f, &self.global, &[])
    }
}

impl Config {
    /// Returns a value whose `Display` produces a full ptp4l config including
    /// one `[<iface>]` section per entry in `interfaces`.
    pub fn display_with_interfaces<'a>(
        &'a self,
        interfaces: &'a [String],
    ) -> impl fmt::Display + 'a {
        struct W<'a> {
            global: &'a GlobalConfig,
            interfaces: &'a [String],
        }
        impl fmt::Display for W<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt_config(f, self.global, self.interfaces)
            }
        }
        W {
            global: &self.global,
            interfaces,
        }
    }
}

/// Write `key  value\n` only when `val` is `Some`.
fn wopt<T: fmt::Display>(f: &mut fmt::Formatter<'_>, key: &str, val: &Option<T>) -> fmt::Result {
    if let Some(v) = val {
        writeln!(f, "{key:<24}{v}")?;
    }
    Ok(())
}

fn fmt_config(f: &mut fmt::Formatter<'_>, g: &GlobalConfig, interfaces: &[String]) -> fmt::Result {
    writeln!(f, "[global]")?;

    // ── Default Data Set ──────────────────────────────────────────────────
    wopt(f, "twoStepFlag", &g.two_step_flag)?;
    wopt(f, "clientOnly", &g.client_only)?;
    wopt(f, "priority1", &g.priority1)?;
    wopt(f, "priority2", &g.priority2)?;
    wopt(f, "domainNumber", &g.domain_number)?;
    wopt(f, "clockClass", &g.clock_class)?;
    if let Some(v) = g.clock_accuracy {
        writeln!(f, "{:<24}{:#04X}", "clockAccuracy", v)?;
    }
    if let Some(v) = g.offset_scaled_log_variance {
        writeln!(f, "{:<24}{:#06X}", "offsetScaledLogVariance", v)?;
    }

    // ── Port Data Set ─────────────────────────────────────────────────────
    wopt(f, "logAnnounceInterval", &g.log_announce_interval)?;
    wopt(f, "logSyncInterval", &g.log_sync_interval)?;
    wopt(f, "logMinDelayReqInterval", &g.log_min_delay_req_interval)?;
    wopt(f, "announceReceiptTimeout", &g.announce_receipt_timeout)?;
    wopt(f, "delayAsymmetry", &g.delay_asymmetry)?;

    // ── Transport ─────────────────────────────────────────────────────────
    if let Some(v) = g.transport_specific {
        writeln!(f, "{:<24}{:#04X}", "transportSpecific", v)?;
    }
    wopt(f, "network_transport", &g.network_transport)?;
    wopt(f, "delay_mechanism", &g.delay_mechanism)?;
    wopt(f, "time_stamping", &g.time_stamping)?;

    // ── Servo ─────────────────────────────────────────────────────────────
    wopt(f, "clock_servo", &g.clock_servo)?;
    wopt(f, "max_frequency", &g.max_frequency)?;
    wopt(f, "step_threshold", &g.step_threshold)?;
    wopt(f, "first_step_threshold", &g.first_step_threshold)?;

    // ── Timestamp processing ──────────────────────────────────────────────
    wopt(f, "tsproc_mode", &g.tsproc_mode)?;
    wopt(f, "delay_filter", &g.delay_filter)?;
    wopt(f, "delay_filter_length", &g.delay_filter_length)?;
    wopt(f, "tx_timestamp_timeout", &g.tx_timestamp_timeout)?;

    // ── Management socket ─────────────────────────────────────────────────
    if let Some(ref v) = g.uds_address {
        writeln!(f, "{:<24}{}", "uds_address", v.display())?;
    }
    if let Some(ref v) = g.uds_ro_address {
        writeln!(f, "{:<24}{}", "uds_ro_address", v.display())?;
    }

    // ── Logging ───────────────────────────────────────────────────────────
    wopt(f, "logging_level", &g.logging_level)?;
    wopt(f, "summary_interval", &g.summary_interval)?;
    wopt(f, "use_syslog", &g.use_syslog)?;
    wopt(f, "verbose", &g.verbose)?;

    // ── Interface sections ────────────────────────────────────────────────
    for iface in interfaces {
        write!(f, "\n[{iface}]")?;
    }

    Ok(())
}

/// Settings that map to the `[global]` section of a ptp4l config file.
///
/// All fields are `Option`; unset fields are omitted from the rendered config
/// and ptp4l falls back to its own built-in defaults for them.
#[derive(Debug, Clone, Default)]
pub struct GlobalConfig {
    // ── Default Data Set ─────────────────────────────────────────────────────
    /// Two-step clock flag (1 = two-step, 0 = one-step).
    pub two_step_flag: Option<u8>,

    /// Run as client/slave only; never become master (1 = client only).
    pub client_only: Option<u8>,

    /// Best master clock priority 1 (lower wins). 255 = never become master.
    pub priority1: Option<u8>,

    /// Best master clock priority 2 (lower wins). 255 = never become master.
    pub priority2: Option<u8>,

    /// PTP domain number. AES67 typically uses 0.
    pub domain_number: Option<u8>,

    /// Clock class (248 = default ordinary clock, 135 = primary reference).
    pub clock_class: Option<u8>,

    /// Clock accuracy (0xFE = unknown).
    pub clock_accuracy: Option<u8>,

    /// Offset scaled log variance (0xFFFF = unknown).
    pub offset_scaled_log_variance: Option<u16>,

    // ── Port Data Set ─────────────────────────────────────────────────────────
    /// Log₂ of the announce interval in seconds (1 → 2 s between announces).
    pub log_announce_interval: Option<i8>,

    /// Log₂ of the sync interval in seconds (0 → 1 sync/s, -3 → 8 sync/s).
    pub log_sync_interval: Option<i8>,

    /// Log₂ of the minimum delay-request interval (0 → 1 req/s).
    pub log_min_delay_req_interval: Option<i8>,

    /// Number of missed announces before the port goes to LISTENING.
    pub announce_receipt_timeout: Option<u8>,

    /// Asymmetric delay correction in nanoseconds.
    pub delay_asymmetry: Option<i64>,

    // ── Transport ─────────────────────────────────────────────────────────────
    /// Upper nibble of the PTP message flags (0x1 for 802.1AS, 0x0 for IEEE 1588).
    pub transport_specific: Option<u8>,

    /// Layer-3/4 transport: `"UDPv4"`, `"UDPv6"`, or `"L2"`.
    pub network_transport: Option<NetworkTransport>,

    /// Delay measurement mechanism: `"E2E"` or `"P2P"`.
    pub delay_mechanism: Option<DelayMechanism>,

    /// Timestamp source: `"hardware"`, `"software"`, or `"legacy"`.
    pub time_stamping: Option<TimeStamping>,

    // ── Servo ─────────────────────────────────────────────────────────────────
    /// Clock servo type: `"pi"` (default) or `"linreg"`.
    pub clock_servo: Option<ClockServo>,

    /// Maximum slew rate in ppb (parts-per-billion).
    pub max_frequency: Option<u64>,

    /// Step the clock if the offset exceeds this threshold (seconds). 0 = never step.
    pub step_threshold: Option<f64>,

    /// Step at start-up if offset exceeds this threshold (seconds).
    pub first_step_threshold: Option<f64>,

    // ── Timestamp processing ──────────────────────────────────────────────────
    /// Timestamp processing mode: `"filter"` or `"raw"`.
    pub tsproc_mode: Option<TsProcMode>,

    /// Delay filter algorithm: `"moving_median"` or `"moving_average"`.
    pub delay_filter: Option<DelayFilter>,

    /// Number of samples in the delay filter.
    pub delay_filter_length: Option<u32>,

    /// Timeout in milliseconds waiting for a TX timestamp from the kernel.
    pub tx_timestamp_timeout: Option<u32>,

    // ── Management socket ─────────────────────────────────────────────────────
    /// Unix domain socket path used by `pmc` and other management clients.
    /// ptp4l's built-in default is `/var/run/ptp4l`.
    pub uds_address: Option<PathBuf>,

    /// Read-only management socket. ptp4l's built-in default is `/var/run/ptp4lro`.
    /// Set to the same directory as `uds_address` to avoid a bind error when
    /// running as a non-root user.
    pub uds_ro_address: Option<PathBuf>,

    // ── Logging ───────────────────────────────────────────────────────────────
    /// Syslog severity level (0 = emergency … 7 = debug). 6 = informational.
    pub logging_level: Option<u8>,

    /// Log₂ of the statistics summary interval in seconds (0 = every second).
    pub summary_interval: Option<i32>,

    /// Write log messages to syslog (1 = yes).
    pub use_syslog: Option<u8>,

    /// Also print log messages to stdout (1 = yes).
    pub verbose: Option<u8>,
}
