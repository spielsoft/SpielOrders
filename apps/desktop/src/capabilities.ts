export type PreferenceValue = boolean | number | string;

export interface PreferencesCapability {
  read(key: string): Promise<PreferenceValue | undefined>;
  write(key: string, value: PreferenceValue): Promise<void>;
}

export interface NativeFilesCapability {
  open(path: string): Promise<void>;
  reveal(path: string): Promise<void>;
}

export interface SupportingMaterialSearchCapability {
  search(query: string, signal: AbortSignal): Promise<void>;
}

/** The single native-capability seam consumed by React code. */
export interface Capabilities {
  preferences: PreferencesCapability;
  nativeFiles: NativeFilesCapability;
  supportingMaterialSearch: SupportingMaterialSearchCapability;
}

/** Ephemeral presentation state stays in the frontend and is never sent to core. */
export class SessionState {
  readonly #values = new Map<string, unknown>();

  read<Value>(key: string): Value | undefined {
    return this.#values.get(key) as Value | undefined;
  }

  write<Value>(key: string, value: Value): void {
    this.#values.set(key, value);
  }
}

