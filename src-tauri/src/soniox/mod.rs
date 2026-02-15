use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

const SONIOX_WSS_HOST: &str = "stt-rt.soniox.com";

#[derive(Debug, Serialize)]
struct SonioxConfig {
    #[serde(rename = "api_key")]
    api_key: String,
    model: String,
    #[serde(rename = "language_hints", skip_serializing_if = "Option::is_none")]
    language_hints: Option<Vec<String>>,
    #[serde(rename = "language_restrictions", skip_serializing_if = "Option::is_none")]
    language_restrictions: Option<Vec<String>>,
    #[serde(rename = "enable_endpoint_detection")]
    enable_endpoint_detection: bool,
    #[serde(rename = "audio_format")]
    audio_format: String,
    #[serde(rename = "sample_rate")]
    sample_rate: u32,
    #[serde(rename = "num_channels")]
    num_channels: u32,
}

#[derive(Debug, Deserialize)]
struct SonioxResponse {
    #[serde(rename = "error_code")]
    error_code: Option<String>,
    #[serde(rename = "error_message")]
    error_message: Option<String>,
    #[serde(rename = "tokens")]
    tokens: Option<Vec<Token>>,
    #[serde(rename = "finished")]
    finished: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct Token {
    #[serde(rename = "text")]
    text: String,
    #[serde(rename = "is_final")]
    is_final: bool,
    #[serde(rename = "speaker")]
    speaker: Option<i32>,
    #[serde(rename = "language")]
    language: Option<String>,
}

pub async fn connect_and_transcribe(
    api_key: String,
    language_hints: Vec<String>,
    language_restrictions: Option<Vec<String>>,
    stop_signal: Arc<AtomicBool>,
    audio_rx: &mut mpsc::Receiver<Vec<u8>>,
    app: tauri::AppHandle,
    target_window_id: String,
) -> Result<(), String> {
    eprintln!("DEBUG: connect_and_transcribe called");

    let url = format!("wss://{}/transcribe-websocket", SONIOX_WSS_HOST);

    log::info!("Connecting to Soniox: {}", url);
    eprintln!("DEBUG: Attempting WebSocket connection to {}", url);

    let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await.map_err(|e| {
        let err_msg = format!("WebSocket connection failed: {}", e);
        eprintln!("DEBUG ERROR: {}", err_msg);
        log::error!("{}", err_msg);
        err_msg
    })?;

    eprintln!("DEBUG: WebSocket connected successfully");
    log::info!("Connected to Soniox");

    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<Message>();

    tokio::spawn(async move {
        while let Some(msg) = ws_rx.recv().await {
            if let Err(e) = ws_write.send(msg).await {
                eprintln!("DEBUG ERROR: WebSocket send failed: {}", e);
                log::error!("WebSocket send failed: {}", e);
                break;
            }
        }
    });

    let config = SonioxConfig {
        api_key: api_key.clone(),
        model: "stt-rt-v4".to_string(),
        language_hints: if language_hints.is_empty() { None } else { Some(language_hints) },
        language_restrictions,
        enable_endpoint_detection: true,
        audio_format: "pcm_s16le".to_string(),
        sample_rate: 16000,
        num_channels: 1,
    };

    let config_json = serde_json::to_string(&config).map_err(|e| e.to_string())?;
    log::info!("Sending config: {}", config_json);

    ws_tx
        .send(Message::Text(config_json))
        .map_err(|e| format!("Failed to queue config: {}", e))?;

    log::info!("Config sent to Soniox");

    // Track the text we've already typed
    let mut typed_text: String = String::new();
    let mut is_transcribing = true;
    let mut audio_channel_closed = false;
    let mut end_signal_sent = false;
    let mut session_finished = false;
    // Track accumulated text for history
    let mut accumulated_text = String::new();

    eprintln!("DEBUG: Starting transcription loop");
    let mut audio_chunks_sent = 0;
    let mut messages_received = 0;

    // Dedicated typing worker so insertion never blocks the transcription loop.
    let (typing_tx, mut typing_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let typing_target_window = target_window_id.clone();
    tokio::spawn(async move {
        while let Some(text) = typing_rx.recv().await {
            let twid = typing_target_window.clone();
            let ttt_for_typing = text.clone();
            let type_result = tokio::task::spawn_blocking(move || {
                crate::typer::type_text(&ttt_for_typing, &twid)
            })
            .await;

            match type_result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    eprintln!("DEBUG ERROR: Failed to type text: {}", e);
                    log::error!("Failed to type text: {}", e);
                }
                Err(e) => {
                    eprintln!("DEBUG ERROR: Typing task failed: {}", e);
                }
            }
        }
    });

    // Use a persistent sleep future to avoid resetting it on every loop iteration.
    // We initialize it with a long duration and reset it when the end signal is sent.
    let finish_timeout = tokio::time::sleep(Duration::from_secs(3600));
    tokio::pin!(finish_timeout);

    // Loop until session is finished or timeout
    while is_transcribing {
        // Check if we should stop (but keep processing until we get final tokens)
        let should_stop = stop_signal.load(Ordering::SeqCst);

        // If user requested stop and we haven't sent end signal yet
        if should_stop && !end_signal_sent {
            eprintln!("DEBUG: Stop requested, sending end signal to Soniox");
            ws_tx.send(Message::Text("".to_string())).ok();
            end_signal_sent = true;
            // Start the 5-second countdown to finish the session
            finish_timeout
                .as_mut()
                .reset(Instant::now() + Duration::from_secs(5));
        }

        // If session is finished, exit after processing remaining messages
        if session_finished {
            eprintln!("DEBUG: Session finished, exiting loop");
            break;
        }

        tokio::select! {
            // Send audio data (or handle closed channel)
            chunk = audio_rx.recv(), if !audio_channel_closed => {
                match chunk {
                    Some(audio_data) => {
                        audio_chunks_sent += 1;
                        if audio_chunks_sent % 100 == 0 {
                            eprintln!("DEBUG: Sent {} audio chunks, latest size: {} bytes", audio_chunks_sent, audio_data.len());
                        }
                        if let Err(e) = ws_tx.send(Message::Binary(audio_data)) {
                            eprintln!("DEBUG ERROR: Failed to send audio: {}", e);
                        }
                    }
                    None => {
                        // Audio channel closed
                        if !audio_channel_closed {
                            eprintln!("DEBUG: Audio channel closed after {} chunks", audio_chunks_sent);
                            if !end_signal_sent {
                                eprintln!("DEBUG: Sending end signal to Soniox");
                                ws_tx.send(Message::Text("".to_string())).ok();
                                end_signal_sent = true;
                                // Start the 5-second countdown to finish the session
                                finish_timeout.as_mut().reset(Instant::now() + Duration::from_secs(5));
                            }
                            audio_channel_closed = true;
                        }
                    }
                }
            }
            // Timeout waiting for final tokens after sending end signal
            _ = &mut finish_timeout, if end_signal_sent && !session_finished => {
                eprintln!("DEBUG: Timeout waiting for final tokens from Soniox");
                is_transcribing = false;
            }
            // Receive transcription results
            msg = ws_read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        messages_received += 1;
                        if messages_received <= 3 {
                            eprintln!("DEBUG: Received message #{}: {}", messages_received, &text[..text.len().min(300)]);
                        }
                        log::debug!("Received message: {}", text);

                        if let Ok(response) = serde_json::from_str::<SonioxResponse>(&text) {
                            // Check for errors
                            if let Some(error_code) = response.error_code {
                                let error_msg = response.error_message.unwrap_or_default();
                                log::error!("Soniox error: {} - {}", error_code, error_msg);
                                app.emit("transcription-error", format!("{} - {}", error_code, error_msg)).ok();
                                break;
                            }

                            // Build full final text from all final tokens
                            let mut final_tokens: Vec<Token> = Vec::new();
                            let mut non_final_tokens: Vec<Token> = Vec::new();

                            if let Some(tokens) = response.tokens {
                                for token in tokens {
                                    if !token.text.is_empty() && !is_control_token(&token.text) {
                                        if token.is_final {
                                            final_tokens.push(token);
                                        } else {
                                            non_final_tokens.push(token);
                                        }
                                    }
                                }
                            }

                            // Build the complete final text
                            let current_final_text: String = final_tokens.iter()
                                .map(|t| t.text.clone())
                                .collect();

                            // Check if we have new text to type
                            let text_to_type = if current_final_text.starts_with(&typed_text) {
                                // Normal case: new text is appended
                                &current_final_text[typed_text.len()..]
                            } else if typed_text.is_empty() {
                                // First batch
                                &current_final_text
                            } else {
                                // Tokens changed! Type the new full text
                                eprintln!("DEBUG WARN: Final text changed! Old: '{}', New: '{}'", typed_text, current_final_text);
                                &current_final_text
                            };

                            if !text_to_type.is_empty() {
                                eprintln!("DEBUG: New text to type: '{}' (total final: '{}')", text_to_type, current_final_text);

                                // Accumulate for history
                                accumulated_text.push_str(text_to_type);

                                // Enqueue typing to the dedicated worker to avoid blocking the loop
                                let ttt_for_emit = text_to_type.to_string();
                                if typing_tx.send(text_to_type.to_string()).is_err() {
                                    eprintln!("DEBUG ERROR: Typing worker channel closed");
                                    log::error!("Typing worker channel closed");
                                }

                                // Update tracking to full current text
                                typed_text = current_final_text.clone();

                                // Emit event with the newly typed text
                                app.emit("transcribed-text", ttt_for_emit).ok();
                            }

                            // Show preview with all final tokens + non-final tokens
                            let preview_non_final: String = non_final_tokens.iter()
                                .map(|t| t.text.clone())
                                .collect();
                            let preview_text = format!("{}{}", current_final_text, preview_non_final);

                            if !preview_text.is_empty() {
                                app.emit("partial-text", preview_text).ok();
                            }

                            // Check if session is finished
                            if response.finished == Some(true) {
                                eprintln!("DEBUG: Session finished flag received");
                                log::info!("Session finished");
                                session_finished = true;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        eprintln!("DEBUG: WebSocket closed by server");
                        log::info!("WebSocket closed by server");
                        is_transcribing = false;
                    }
                    Some(Err(e)) => {
                        eprintln!("DEBUG ERROR: WebSocket error: {}", e);
                        log::error!("WebSocket error: {}", e);
                        app.emit("transcription-error", e.to_string()).ok();
                    }
                    None => {
                        eprintln!("DEBUG: WebSocket stream ended");
                        log::info!("WebSocket stream ended");
                        is_transcribing = false;
                    }
                    _ => {}
                }
            }
        }
    }

    // Emit the complete accumulated text for history
    if !accumulated_text.is_empty() {
        eprintln!(
            "DEBUG: Emitting session-complete with {} chars",
            accumulated_text.len()
        );
        app.emit("session-complete", accumulated_text).ok();
    }

    log::info!("Transcription ended");
    Ok(())
}

/// Returns true if the token text is a Soniox control/special token
/// like <end>, <laugh>, <noise>, etc. that should not be typed.
fn is_control_token(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with('<') && trimmed.ends_with('>')
}
