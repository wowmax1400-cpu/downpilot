import { Dialog, Btn } from "./Dialog";

export function ShortcutsDialog({ onClose }: { onClose: () => void }) {
  const shortcuts = [
    { key: "Ctrl + N", desc: "Add new download" },
    { key: "Ctrl + F", desc: "Focus search box" },
    { key: "Ctrl + A", desc: "Select all downloads" },
    { key: "Ctrl + P", desc: "Download properties" },
    { key: "Space", desc: "Resume / Pause selected download" },
    { key: "Delete", desc: "Delete selected download" },
    { key: "F2", desc: "Rename download" },
    { key: "Esc", desc: "Close dialog / modal" },
  ];

  return (
    <Dialog title="Keyboard Shortcuts" onClose={onClose} width={420}
      footer={<Btn variant="primary" onClick={onClose}>Close</Btn>}>
      <div style={{ display: "flex", flexDirection: "column", gap: 6, padding: "4px 0" }}>
        {shortcuts.map((s) => (
          <div key={s.key} style={{ display: "flex", justifyContent: "space-between", alignItems: "center", borderBottom: "1px solid #ECECEC", padding: "4px 2px", fontSize: 11 }}>
            <kbd style={{ background: "#F1F5F9", border: "1px solid #CBD5E1", borderRadius: 3, padding: "2px 6px", fontFamily: "monospace", fontWeight: 600 }}>{s.key}</kbd>
            <span style={{ color: "#334155" }}>{s.desc}</span>
          </div>
        ))}
      </div>
    </Dialog>
  );
}
