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

use miette::Result;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // let config = Config::load().await?;
    // telemetry::init(&config).await?;

    // let id = config.instance_name();
    // let rx_config = config
    //     .receiver_config
    //     .as_ref()
    //     .expect("no receiver config")
    //     .to_owned();

    // info!(
    //     "Starting {} instance '{}' with session description:\n{}",
    //     config.app.name,
    //     config.app.instance.name,
    //     rx_config.session.marshal()
    // );

    // Toplevel::new(move |s| async move {
    //     s.start(SubsystemBuilder::new("aes67-vsc-2", move |s| async move {
    //         // let wb = start_worterbuch(&s, config.clone()).await.ok();
    //         let descriptor = RxDescriptor::try_from(&rx_config)?;

    //         let (req_serv, req_client) = request_response_channel();

    //         // let ptp_clock =
    //         //     StatimePtpMediaClock::new(&config, descriptor.audio_format, wb.clone()).await?;
    //         let system_clock = SystemMediaClock::new(descriptor.audio_format);
    //         let compensate_clock_drift = false;

    //         let receiver_api = start_receiver(
    //             id,
    //             &s,
    //             rx_config.clone(),
    //             false,
    //             // wb.clone(),
    //             None,
    //             system_clock.clone(),
    //             req_serv,
    //         )
    //         .await?;
    //         info!("Receiver API running at {}", receiver_api.url());

    //         let playout_api = start_jack_playout(
    //             &s,
    //             config.clone(),
    //             false,
    //             // wb.clone(),
    //             None,
    //             system_clock.clone(),
    //             compensate_clock_drift,
    //             req_client,
    //         )
    //         .await?;
    //         info!("Playout API running at {}", playout_api.url());

    //         s.start(SubsystemBuilder::new("clock-logger", |s| async move {
    //             let mut interval = tokio::time::interval(Duration::from_secs(
    //                 config
    //                     .playout_config
    //                     .as_ref()
    //                     .map(|c| c.clock_drift_compensation_interval)
    //                     .unwrap_or(1) as u64,
    //             ));
    //             loop {
    //                 select! {
    //                     _ = interval.tick() => print_time(&system_clock)?,
    //                     _ = s.on_shutdown_requested() => break,
    //                 }
    //             }

    //             Ok::<(), Aes67Vsc2Error>(())
    //         }));

    //         Ok::<(), Aes67Vsc2Error>(())
    //     }));
    // })
    // .catch_signals()
    // .handle_shutdown_requests(Duration::from_secs(1))
    // .await?;

    Ok(())
}

// fn print_time<C: MediaClock>(ptp_clock: &C) -> Aes67Vsc2Result<()> {
//     let ptp_time = ptp_clock.current_ptp_time_millis()?;
//     info!("PTP time: {}", datetime(ptp_time),);
//     Ok(())
// }

// fn datetime(now: u64) -> DateTime<Local> {
//     DateTime::<Local>::from(UNIX_EPOCH + Duration::from_millis(now))
// }
