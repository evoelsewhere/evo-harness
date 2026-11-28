// Evo Mates' window preferences (docs/features/evo-mates-ui-v2/spec.md,
// FR-1, FR-3, FR-30; plan 3.4): whether the sidebar is folded to its rail,
// which sidebar sections are folded, and how Files lays out. Kept in this
// window's local storage like the app's other UI preferences.

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export type FilesView = "grid" | "list";

interface MatesUiStore {
  sidebarFolded: boolean;
  foldedSections: string[];
  filesView: FilesView;
  /** The quick switcher (FR-7) and the shortcuts sheet (FR-8) are open. */
  switcherOpen: boolean;
  shortcutsOpen: boolean;
  /** The find bar (FR-15) is open over the shown conversation or channel. */
  findOpen: boolean;
  /** A channel's members list (FR-19), opened from its faces or Ctrl+I. */
  membersOpen: boolean;
  /** A window dialog opened from anywhere (the sidebar, the switcher, an empty state). */
  dialog: "new-channel" | "import" | null;
  toggleSidebar(): void;
  toggleSection(id: string): void;
  setFilesView(view: FilesView): void;
  setSwitcherOpen(open: boolean): void;
  setShortcutsOpen(open: boolean): void;
  setFindOpen(open: boolean): void;
  setMembersOpen(open: boolean): void;
  setDialog(dialog: "new-channel" | "import" | null): void;
}

function storage() {
  try {
    return localStorage;
  } catch {
    // Tests run without a DOM: keep it in memory.
    const map = new Map<string, string>();
    return {
      getItem: (key: string) => map.get(key) ?? null,
      setItem: (key: string, value: string) => void map.set(key, value),
      removeItem: (key: string) => void map.delete(key),
    };
  }
}

export const useMatesUiStore = create<MatesUiStore>()(
  persist(
    (set) => ({
      sidebarFolded: false,
      foldedSections: [],
      filesView: "grid",
      switcherOpen: false,
      shortcutsOpen: false,
      findOpen: false,
      membersOpen: false,
      dialog: null,
      toggleSidebar: () => set((s) => ({ sidebarFolded: !s.sidebarFolded })),
      toggleSection: (id) =>
        set((s) => ({
          foldedSections: s.foldedSections.includes(id) ? s.foldedSections.filter((x) => x !== id) : [...s.foldedSections, id],
        })),
      setFilesView: (filesView) => set({ filesView }),
      setSwitcherOpen: (switcherOpen) => set({ switcherOpen }),
      setShortcutsOpen: (shortcutsOpen) => set({ shortcutsOpen }),
      setFindOpen: (findOpen) => set({ findOpen }),
      setMembersOpen: (membersOpen) => set({ membersOpen }),
      setDialog: (dialog) => set({ dialog }),
    }),
    {
      name: "evo.mates-ui",
      version: 0,
      storage: createJSONStorage(storage),
      // Only the preferences are kept; open dialogs are not.
      partialize: (s) => ({ sidebarFolded: s.sidebarFolded, foldedSections: s.foldedSections, filesView: s.filesView }),
    },
  ),
);
