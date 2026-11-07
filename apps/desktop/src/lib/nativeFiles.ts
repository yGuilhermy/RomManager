import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export type PathChoice = {
  title: string;
  directory?: boolean;
  extensions?: string[];
  current?: string;
};

function requireDesktop() {
  if (!isTauri()) throw new Error("Abra o RomManager desktop para usar o seletor de arquivos do sistema.");
}

export async function choosePath(options: PathChoice): Promise<string | null> {
  requireDesktop();
  return await open({
    title: options.title,
    directory: options.directory ?? false,
    multiple: false,
    defaultPath: options.current?.trim().replace(/^["']|["']$/g, "") || undefined,
    filters: options.extensions ? [{ name: options.title, extensions: options.extensions }] : undefined,
  });
}

export async function saveGeneratedFile(request: { kind: "save"; source: string } | { kind: "profile"; contents: string }): Promise<string | null> {
  requireDesktop();
  return await invoke<string | null>("save_generated_file", { request });
}
