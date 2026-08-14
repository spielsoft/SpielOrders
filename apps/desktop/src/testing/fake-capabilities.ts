import type { Capabilities, PreferenceValue } from "../capabilities";

export class FakeCapabilities implements Capabilities {
  readonly values = new Map<string, PreferenceValue>();
  readonly opened: string[] = [];
  readonly revealed: string[] = [];
  readonly searches: string[] = [];

  readonly preferences = {
    read: async (key: string) => this.values.get(key),
    write: async (key: string, value: PreferenceValue) => {
      this.values.set(key, value);
    }
  };

  readonly nativeFiles = {
    open: async (path: string) => {
      this.opened.push(path);
    },
    reveal: async (path: string) => {
      this.revealed.push(path);
    }
  };

  readonly supportingMaterialSearch = {
    search: async (query: string, signal: AbortSignal) => {
      if (signal.aborted) throw signal.reason;
      this.searches.push(query);
    }
  };
}

