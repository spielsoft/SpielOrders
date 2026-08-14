import { invoke } from "@tauri-apps/api/core";
import type { Capabilities, PreferenceValue } from "../capabilities";

export const tauriCapabilities: Capabilities = {
  preferences: {
    read: (key) => invoke<PreferenceValue | undefined>("preferences_read", { key }),
    write: (key, value) => invoke<void>("preferences_write", { key, value })
  },
  nativeFiles: {
    open: (path) => invoke<void>("native_file_open", { path }),
    reveal: (path) => invoke<void>("native_file_reveal", { path })
  },
  supportingMaterialSearch: {
    search: (query, signal) => {
      if (signal.aborted) return Promise.reject(signal.reason);
      return invoke<void>("supporting_material_search", { query });
    }
  }
};

