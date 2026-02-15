use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use tokio::sync::mpsc;

// Target format for Soniox
const TARGET_SAMPLE_RATE: u32 = 16000;
const TARGET_CHANNELS: u16 = 1;

pub async fn start_audio_capture(
    api_key: String,
    language_hints: Vec<String>,
    language_restrictions: Option<Vec<String>>,
    stop_signal: Arc<AtomicBool>,
    app: tauri::AppHandle,
    target_window_id: String,
) -> Result<(), String> {
    log::info!("Initializing audio capture...");
    
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(100);

    // Get default input device
    let host = cpal::default_host();
    let device = match host.default_input_device() {
        Some(d) => d,
        None => {
            let err = "No input device available".to_string();
            log::error!("{}", err);
            return Err(err);
        }
    };
    
    log::info!("Using audio device: {:?}", device.name());

    // Try to get the default config first to see what the device supports
    let default_config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            let err = format!("No default input config: {}", e);
            log::error!("{}", err);
            return Err(err);
        }
    };
    
    // Build a config with our target sample rate
    let mut config: StreamConfig = default_config.config();
    config.sample_rate.0 = TARGET_SAMPLE_RATE;
    config.channels = TARGET_CHANNELS;
    
    log::info!("Audio config: sample_rate={:?}, channels={:?}", config.sample_rate, config.channels);

    // Spawn audio capture in a separate thread
    let stop_flag_for_thread = stop_signal.clone();
    let audio_thread = std::thread::spawn(move || {
        let err_fn = |err| log::error!("Audio stream error: {}", err);

        let stream_result: Result<Stream, cpal::BuildStreamError> = match default_config.sample_format() {
            SampleFormat::F32 => {
                let tx_clone = tx.clone();
                let stop = stop_flag_for_thread.clone();
                let data_callback = move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    
                    // Convert f32 to i16
                    let pcm_data: Vec<u8> = data
                        .iter()
                        .flat_map(|&sample| {
                            let sample_i16 = (sample * 32767.0_f32) as i16;
                            sample_i16.to_le_bytes()
                        })
                        .collect();
                    
                    if !pcm_data.is_empty() {
                        tx_clone.blocking_send(pcm_data).ok();
                    }
                };
                device.build_input_stream(&config, data_callback, err_fn, None)
            }
            SampleFormat::I16 => {
                let tx_clone = tx.clone();
                let stop = stop_flag_for_thread.clone();
                let data_callback = move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    
                    let pcm_data: Vec<u8> = data
                        .iter()
                        .flat_map(|&sample| sample.to_le_bytes())
                        .collect();
                    
                    if !pcm_data.is_empty() {
                        tx_clone.blocking_send(pcm_data).ok();
                    }
                };
                device.build_input_stream(&config, data_callback, err_fn, None)
            }
            SampleFormat::U16 => {
                let tx_clone = tx.clone();
                let stop = stop_flag_for_thread.clone();
                let data_callback = move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    
                    let pcm_data: Vec<u8> = data
                        .iter()
                        .flat_map(|&sample| {
                            let sample_i16 = (sample as i32 - 32768) as i16;
                            sample_i16.to_le_bytes()
                        })
                        .collect();
                    
                    if !pcm_data.is_empty() {
                        tx_clone.blocking_send(pcm_data).ok();
                    }
                };
                device.build_input_stream(&config, data_callback, err_fn, None)
            }
            _ => {
                log::error!("Unsupported sample format");
                return Err("Unsupported sample format".to_string());
            }
        };

        let stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to build audio stream: {}", e);
                return Err(format!("Failed to build audio stream: {}", e));
            }
        };
        
        if let Err(e) = stream.play() {
            log::error!("Failed to start audio stream: {}", e);
            return Err(format!("Failed to start audio stream: {}", e));
        }
        
        log::info!("Audio capture started successfully");
        
        // Keep thread alive until stop signal
        while !stop_flag_for_thread.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        
        drop(stream);
        log::info!("Audio capture stopped");
        Ok(())
    });
    
    // Run transcription
    let result = crate::soniox::connect_and_transcribe(api_key, language_hints, language_restrictions, stop_signal.clone(), &mut rx, app, target_window_id).await;
    
    // Signal audio capture to stop (in case it hasn't already)
    stop_signal.store(true, Ordering::SeqCst);
    
    // Wait for audio thread to finish
    if let Err(e) = audio_thread.join() {
        log::error!("Audio thread panicked: {:?}", e);
    }
    
    result
}
