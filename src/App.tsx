import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Settings from "./components/Settings";

interface AppSettings {
  api_key: string;
  hotkey: string;
  language_hints: string[];
  language_restrictions: string[] | null;
}

interface TranscriptionEntry {
  text: string;
  timestamp: number;
  language: string;
}

type View = "home" | "settings" | "history";

function App() {
  const [settings, setSettings] = useState<AppSettings>({
    api_key: "",
    hotkey: "",
    language_hints: ["en"],
    language_restrictions: null,
  });
  const [isRecording, setIsRecording] = useState(false);
  const isRecordingRef = useRef(false);
  const [currentView, setCurrentView] = useState<View>("home");
  const [partialText, setPartialText] = useState("");
  const [sessionText, setSessionText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<TranscriptionEntry[]>([]);
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);

  useEffect(() => {
    loadSettings();
    loadHistory();
    setupEventListeners();
    return () => {
      cleanupEventListeners();
    };
  }, []);

  async function loadSettings() {
    try {
      const s = await invoke<AppSettings>("get_settings");
      setSettings(s);
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  }

  async function loadHistory() {
    try {
      const entries = await invoke<TranscriptionEntry[]>("get_transcriptions");
      setHistory(entries);
    } catch (e) {
      console.error("Failed to load transcription history:", e);
    }
  }

  async function setupEventListeners() {
    await listen("recording-started", () => {
      setIsRecording(true);
      isRecordingRef.current = true;
      setError(null);
      setSessionText("");
    });

    await listen("recording-stopped", () => {
      setIsRecording(false);
      isRecordingRef.current = false;
      // The session-complete event will handle saving
    });

    await listen("partial-text", (event) => {
      setPartialText(event.payload as string);
    });

    await listen("transcribed-text", () => {
      // transcribed-text is a delta event used for typing only.
      // Preview display is driven by partial-text (during recording)
      // and session-complete (after recording).
    });

    await listen("session-complete", (event) => {
      const text = event.payload as string;
      // Clean up whitespace and save to history
      const cleanedText = text
        .replace(/\s+/g, ' ')  // Normalize multiple spaces
        .replace(/ ([.,!?;:])/g, '$1')  // Remove space before punctuation
        .trim();
      if (cleanedText) {
        setSessionText(cleanedText);
        saveTranscription(cleanedText);
      }
      setPartialText("");
    });

    await listen("transcription-error", (event) => {
      setError(event.payload as string);
      setIsRecording(false);
    });

    await listen("recording-error", (event) => {
      setError(event.payload as string);
      setIsRecording(false);
    });
  }

  function cleanupEventListeners() {
    // Cleanup handled by Tauri
  }

  async function saveTranscription(text: string) {
    try {
      await invoke("save_transcription", { text, languageHints: settings.language_hints });
      await loadHistory();
    } catch (e) {
      console.error("Failed to save transcription:", e);
    }
  }

  async function clearHistory() {
    try {
      await invoke("clear_transcriptions");
      setHistory([]);
    } catch (e) {
      console.error("Failed to clear history:", e);
    }
  }

  async function startRecording() {
    try {
      setError(null);
      await invoke("start_recording");
    } catch (e) {
      setError(e as string);
    }
  }

  async function stopRecording() {
    try {
      await invoke("stop_recording");
    } catch (e) {
      console.error("Failed to stop recording:", e);
    }
  }

  async function handleSaveSettings(newSettings: AppSettings) {
    setSettings(newSettings);
    try {
      await invoke("save_settings", { settings: newSettings });
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
    setCurrentView("home");
  }

  function copyToClipboard(text: string, index: number) {
    navigator.clipboard.writeText(text).then(() => {
      setCopiedIndex(index);
      setTimeout(() => setCopiedIndex(null), 1500);
    });
  }

  function formatTimestamp(ts: number): string {
    const date = new Date(ts * 1000);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return "Just now";
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    return date.toLocaleDateString();
  }

  return (
    <div className="app">
      <header className="header">
        <h1>Desktop Dictate</h1>
        <div className="header-actions">
          <button
            className={`nav-btn ${currentView === "history" ? "active" : ""}`}
            onClick={() => setCurrentView(currentView === "history" ? "home" : "history")}
            title="Transcription History"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="10"/>
              <polyline points="12 6 12 12 16 14"/>
            </svg>
          </button>
          <button
            className={`nav-btn ${currentView === "settings" ? "active" : ""}`}
            onClick={() => setCurrentView(currentView === "settings" ? "home" : "settings")}
            title="Settings"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
          </button>
        </div>
      </header>

      {currentView === "settings" ? (
        <Settings
          settings={settings}
          onSave={handleSaveSettings}
          onCancel={() => setCurrentView("home")}
        />
      ) : currentView === "history" ? (
        <div className="history">
          <div className="history-header">
            <h2>Transcription History</h2>
            {history.length > 0 && (
              <button className="clear-history-btn" onClick={clearHistory}>
                Clear All
              </button>
            )}
          </div>
          {history.length === 0 ? (
            <div className="history-empty">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#ccc" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                <polyline points="14 2 14 8 20 8"/>
                <line x1="16" y1="13" x2="8" y2="13"/>
                <line x1="16" y1="17" x2="8" y2="17"/>
              </svg>
              <p>No transcriptions yet</p>
              <p className="history-empty-sub">Your dictation history will appear here</p>
            </div>
          ) : (
            <div className="history-list">
              {history.map((entry, i) => (
                <div key={`${entry.timestamp}-${i}`} className="history-item">
                  <div className="history-item-header">
                    <span className="history-time">{formatTimestamp(entry.timestamp)}</span>
                    <span className="history-lang">{entry.language || "N/A"}</span>
                  </div>
                  <div className="history-text">{entry.text}</div>
                  <button
                    className={`history-copy-btn ${copiedIndex === i ? "copied" : ""}`}
                    onClick={() => copyToClipboard(entry.text, i)}
                  >
                    {copiedIndex === i ? "Copied" : "Copy"}
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      ) : (
        <main className="main">
          <div className="status">
            <div className={`status-indicator ${isRecording ? "recording" : ""}`} />
            <span>{isRecording ? "Recording..." : "Ready"}</span>
          </div>

          <div className="hotkey-display">
            <span className="label">Hotkey:</span>
            <span className="value">{settings.hotkey}</span>
          </div>

          <div className="language-display">
            <span className="label">Language Hints:</span>
            <span className="value">{settings.language_hints.length > 0 ? settings.language_hints.join(", ") : "Auto-detect"}</span>
          </div>

          {error && <div className="error-message">{error}</div>}

          <div className="preview-area">
            <div className="preview-label">{isRecording ? "Recording..." : sessionText ? "Last Session:" : "Preview:"}</div>
            <div className="preview-text">
              {isRecording 
                ? partialText || "Listening..."
                : sessionText 
                  ? sessionText 
                  : "Press hotkey to start dictating..."}
            </div>
          </div>

          <button
            className={`record-btn ${isRecording ? "recording" : ""}`}
            onClick={isRecording ? stopRecording : startRecording}
          >
            {isRecording ? "Stop" : "Start Dictation"}
          </button>
        </main>
      )}
    </div>
  );
}

export default App;
