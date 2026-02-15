import { useState, useEffect, useRef, useCallback } from "react";

interface AppSettings {
  api_key: string;
  hotkey: string;
  language_hints: string[];
  language_restrictions: string[] | null;
}

interface SettingsProps {
  settings: AppSettings;
  onSave: (settings: AppSettings) => void;
  onCancel: () => void;
}

const LANGUAGES = [
  { code: "af", name: "Afrikaans" },
  { code: "sq", name: "Albanian" },
  { code: "ar", name: "Arabic" },
  { code: "az", name: "Azerbaijani" },
  { code: "eu", name: "Basque" },
  { code: "be", name: "Belarusian" },
  { code: "bn", name: "Bengali" },
  { code: "bs", name: "Bosnian" },
  { code: "bg", name: "Bulgarian" },
  { code: "ca", name: "Catalan" },
  { code: "zh", name: "Chinese" },
  { code: "hr", name: "Croatian" },
  { code: "cs", name: "Czech" },
  { code: "da", name: "Danish" },
  { code: "nl", name: "Dutch" },
  { code: "en", name: "English" },
  { code: "et", name: "Estonian" },
  { code: "fi", name: "Finnish" },
  { code: "fr", name: "French" },
  { code: "gl", name: "Galician" },
  { code: "de", name: "German" },
  { code: "el", name: "Greek" },
  { code: "gu", name: "Gujarati" },
  { code: "he", name: "Hebrew" },
  { code: "hi", name: "Hindi" },
  { code: "hu", name: "Hungarian" },
  { code: "id", name: "Indonesian" },
  { code: "it", name: "Italian" },
  { code: "ja", name: "Japanese" },
  { code: "kn", name: "Kannada" },
  { code: "kk", name: "Kazakh" },
  { code: "ko", name: "Korean" },
  { code: "lv", name: "Latvian" },
  { code: "lt", name: "Lithuanian" },
  { code: "mk", name: "Macedonian" },
  { code: "ms", name: "Malay" },
  { code: "ml", name: "Malayalam" },
  { code: "mr", name: "Marathi" },
  { code: "no", name: "Norwegian" },
  { code: "fa", name: "Persian" },
  { code: "pl", name: "Polish" },
  { code: "pt", name: "Portuguese" },
  { code: "pa", name: "Punjabi" },
  { code: "ro", name: "Romanian" },
  { code: "ru", name: "Russian" },
  { code: "sr", name: "Serbian" },
  { code: "sk", name: "Slovak" },
  { code: "sl", name: "Slovenian" },
  { code: "es", name: "Spanish" },
  { code: "sw", name: "Swahili" },
  { code: "sv", name: "Swedish" },
  { code: "tl", name: "Tagalog" },
  { code: "ta", name: "Tamil" },
  { code: "te", name: "Telugu" },
  { code: "th", name: "Thai" },
  { code: "tr", name: "Turkish" },
  { code: "uk", name: "Ukrainian" },
  { code: "ur", name: "Urdu" },
  { code: "vi", name: "Vietnamese" },
  { code: "cy", name: "Welsh" },
];

const HOTKEY_PRESETS = [
  { value: "Insert", label: "Insert" },
  { value: "F1", label: "F1" },
  { value: "F2", label: "F2" },
  { value: "F3", label: "F3" },
  { value: "F4", label: "F4" },
  { value: "F5", label: "F5" },
  { value: "F6", label: "F6" },
  { value: "F7", label: "F7" },
  { value: "F8", label: "F8" },
  { value: "F9", label: "F9" },
  { value: "F10", label: "F10" },
  { value: "F11", label: "F11" },
  { value: "F12", label: "F12" },
  { value: "Home", label: "Home" },
  { value: "End", label: "End" },
  { value: "CommandOrControl+Shift+D", label: "Ctrl+Shift+D" },
  { value: "CommandOrControl+Shift+S", label: "Ctrl+Shift+S" },
  { value: "CommandOrControl+Shift+V", label: "Ctrl+Shift+V" },
  { value: "CommandOrControl+Shift+Space", label: "Ctrl+Shift+Space" },
  { value: "CommandOrControl+Alt+Space", label: "Ctrl+Alt+Space" },
  { value: "CommandOrControl+Alt+Shift+D", label: "Ctrl+Alt+Shift+D" },
];

interface MultiSelectProps {
  options: { code: string; name: string }[];
  selected: string[];
  onChange: (selected: string[]) => void;
  placeholder: string;
}

function MultiSelect({ options, selected, onChange, placeholder }: MultiSelectProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [searchTerm, setSearchTerm] = useState("");
  const [openUpward, setOpenUpward] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const handleClickOutside = useCallback((event: MouseEvent) => {
    if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
      setIsOpen(false);
    }
  }, []);

  const handleEscape = useCallback((event: KeyboardEvent) => {
    if (event.key === "Escape") {
      setIsOpen(false);
    }
  }, []);

  useEffect(() => {
    if (isOpen) {
      document.addEventListener("mousedown", handleClickOutside);
      document.addEventListener("keydown", handleEscape);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [isOpen, handleClickOutside, handleEscape]);

  const toggleOpen = () => {
    if (!isOpen && containerRef.current) {
      const rect = containerRef.current.getBoundingClientRect();
      const spaceBelow = window.innerHeight - rect.bottom;
      const menuHeight = 320; // max-height of .multi-select-menu
      setOpenUpward(spaceBelow < menuHeight && rect.top > spaceBelow);
    }
    setIsOpen(!isOpen);
  };

  const toggleOption = (code: string) => {
    if (selected.includes(code)) {
      onChange(selected.filter((s) => s !== code));
    } else {
      onChange([...selected, code]);
    }
  };

  const filteredOptions = options.filter((opt) =>
    opt.name.toLowerCase().includes(searchTerm.toLowerCase())
  );

  const selectedNames = selected.map((code) => options.find((o) => o.code === code)?.name || code);

  return (
    <div className="multi-select-container" ref={containerRef}>
      <div className="multi-select-trigger" onClick={toggleOpen}>
        <span className={selected.length === 0 ? "placeholder" : ""}>
          {selected.length === 0 ? placeholder : selectedNames.join(", ")}
        </span>
        <span className={`dropdown-chevron ${isOpen ? "open" : ""}`}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </span>
      </div>

      {isOpen && (
        <div className={`multi-select-menu ${openUpward ? "open-upward" : ""}`}>
          <div className="menu-header">
            <span className="selected-count">
              {selected.length === 0 ? "No selection" : `${selected.length} selected`}
            </span>
            <div className="header-actions">
              <button
                type="button"
                className="action-btn"
                onClick={(e) => {
                  e.stopPropagation();
                  onChange([]);
                }}
              >
                Clear
              </button>
              <button
                type="button"
                className="action-btn"
                onClick={(e) => {
                  e.stopPropagation();
                  onChange(options.map((o) => o.code));
                }}
              >
                All
              </button>
            </div>
          </div>

          <div className="menu-search">
            <input
              type="text"
              placeholder="Search..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              autoFocus
            />
          </div>

          <div className="menu-options">
            {filteredOptions.map((opt) => (
              <label key={opt.code} className="option-item">
                <input
                  type="checkbox"
                  checked={selected.includes(opt.code)}
                  onChange={() => toggleOption(opt.code)}
                />
                <span className="checkmark">
                  {selected.includes(opt.code) && (
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3">
                      <polyline points="20 6 9 17 4 12" />
                    </svg>
                  )}
                </span>
                <span className="option-name">{opt.name}</span>
              </label>
            ))}
            {filteredOptions.length === 0 && (
              <div className="no-results">No languages found</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function Settings({ settings, onSave, onCancel }: SettingsProps) {
  const [apiKey, setApiKey] = useState(settings.api_key);
  const [hotkey, setHotkey] = useState(settings.hotkey);
  const [languageHints, setLanguageHints] = useState(settings.language_hints);
  const [languageRestrictions, setLanguageRestrictions] = useState<string[]>(settings.language_restrictions || []);
  const [useRestrictions, setUseRestrictions] = useState(!!settings.language_restrictions);
  const [showApiKey, setShowApiKey] = useState(false);

  const [hotkeyMode, setHotkeyMode] = useState<"preset" | "custom">("preset");
  const [isRecording, setIsRecording] = useState(false);
  const [recordedKeys, setRecordedKeys] = useState<string[]>([]);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    onSave({
      api_key: apiKey,
      hotkey,
      language_hints: languageHints,
      language_restrictions: useRestrictions && languageRestrictions.length > 0 ? languageRestrictions : null,
    });
  }

  function startRecording() {
    setIsRecording(true);
    setRecordedKeys([]);
  }

  function stopRecording() {
    if (recordedKeys.length > 0) {
      setHotkey(recordedKeys.join("+"));
    }
    setIsRecording(false);
  }

  function handleHotkeyKeyDown(e: React.KeyboardEvent) {
    if (!isRecording) return;

    e.preventDefault();
    e.stopPropagation();

    const keys: string[] = [];
    if (e.ctrlKey || e.metaKey) keys.push("CommandOrControl");
    if (e.altKey) keys.push("Alt");
    if (e.shiftKey) keys.push("Shift");

    let keyName = e.key;
    if (keyName === " ") keyName = "Space";
    if (keyName !== "Control" && keyName !== "Alt" && keyName !== "Shift" && keyName !== "Meta") {
      keys.push(keyName);
    }

    if (keys.length > 0) {
      setRecordedKeys(keys);
    }
  }

  const isCustomHotkey = !HOTKEY_PRESETS.some((h) => h.value === hotkey);
  const displayHotkey = isCustomHotkey ? hotkey : HOTKEY_PRESETS.find((h) => h.value === hotkey)?.label || hotkey;

  return (
    <div className="settings">
      <h2>Settings</h2>
      <form onSubmit={handleSubmit}>
        <div className="form-group">
          <label>Soniox API Key</label>
          <div className="api-key-input">
            <input type={showApiKey ? "text" : "password"} value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="Enter your Soniox API key" />
            <button type="button" className="toggle-visibility" onClick={() => setShowApiKey(!showApiKey)}>
              {showApiKey ? "Hide" : "Show"}
            </button>
          </div>
          <a href="https://soniox.com/get-started" target="_blank" rel="noopener noreferrer" className="help-link">
            Get API key from Soniox
          </a>
        </div>

        <div className="form-group">
          <label>Global Hotkey</label>

          <div className="hotkey-section">
            <div className="hotkey-mode-tabs">
              <button
                type="button"
                className={`hotkey-tab ${hotkeyMode === "preset" ? "active" : ""}`}
                onClick={() => {
                  setHotkeyMode("preset");
                  setIsRecording(false);
                }}
              >
                Preset
              </button>
              <button
                type="button"
                className={`hotkey-tab ${hotkeyMode === "custom" ? "active" : ""}`}
                onClick={() => {
                  setHotkeyMode("custom");
                  setRecordedKeys([]);
                }}
              >
                Custom
              </button>
            </div>

            {hotkeyMode === "preset" ? (
              <select
                value={HOTKEY_PRESETS.some((h) => h.value === hotkey) ? hotkey : "custom"}
                onChange={(e) => {
                  if (e.target.value === "custom") {
                    setHotkeyMode("custom");
                  } else {
                    setHotkey(e.target.value);
                  }
                }}
              >
                {HOTKEY_PRESETS.map((key) => (
                  <option key={key.value} value={key.value}>
                    {key.label}
                  </option>
                ))}
                <option value="custom">Custom...</option>
              </select>
            ) : (
              <div className="hotkey-custom">
                <div
                  className={`hotkey-recorder ${isRecording ? "recording" : ""} ${isCustomHotkey && !isRecording ? "custom" : ""}`}
                  onKeyDown={handleHotkeyKeyDown}
                  tabIndex={0}
                >
                  {isRecording ? (
                    <div className="recorder-content">
                      <span className="recorder-status">
                        {recordedKeys.length > 0 ? recordedKeys.join(" + ") : "Press any keys..."}
                      </span>
                      <div className="recorder-actions">
                        <button
                          type="button"
                          className="recorder-btn save"
                          onClick={(e) => {
                            e.stopPropagation();
                            stopRecording();
                          }}
                          disabled={recordedKeys.length === 0}
                        >
                          Save
                        </button>
                        <button
                          type="button"
                          className="recorder-btn cancel"
                          onClick={(e) => {
                            e.stopPropagation();
                            setIsRecording(false);
                            setRecordedKeys([]);
                          }}
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="recorder-content">
                      <span className="current-hotkey">{displayHotkey}</span>
                      <button
                        type="button"
                        className="recorder-btn record"
                        onClick={(e) => {
                          e.stopPropagation();
                          startRecording();
                        }}
                      >
                        Record
                      </button>
                    </div>
                  )}
                </div>
                <p className="hotkey-help">Click "Record" then press any key combination. Press "Save" when done.</p>
              </div>
            )}
          </div>
        </div>

        <div className="form-group">
          <label>Language Hints (optional)</label>
          <p className="field-help">Select languages that might be used in your dictation. Leave empty for automatic detection.</p>
          <MultiSelect
            options={LANGUAGES}
            selected={languageHints}
            onChange={setLanguageHints}
            placeholder="Select languages..."
          />
        </div>

        <div className="form-group">
          <label className="checkbox-label">
            <input type="checkbox" checked={useRestrictions} onChange={(e) => setUseRestrictions(e.target.checked)} />
            Enable Language Restrictions
          </label>
          <p className="field-help">Restrict recognition to only the selected languages.</p>
          {useRestrictions && (
            <MultiSelect
              options={LANGUAGES}
              selected={languageRestrictions}
              onChange={setLanguageRestrictions}
              placeholder="Select restricted languages..."
            />
          )}
        </div>

        <div className="form-actions">
          <button type="button" className="cancel-btn" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit" className="save-btn">
            Save
          </button>
        </div>
      </form>
    </div>
  );
}

export default Settings;
