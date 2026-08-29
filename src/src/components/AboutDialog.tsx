import { Dialog, Btn } from "./Dialog";

export function AboutDialog({ onClose }: { onClose: () => void }) {
  return (
    <Dialog title="About DownPilot" onClose={onClose} width={460}
      footer={<Btn variant="primary" onClick={onClose}>OK</Btn>}>
      <div className="about">
        <div className="about__logo">⬇</div>
        <h2 className="about__name">DownPilot</h2>
        <div className="about__ver">Version 1.0.0</div>
        <p className="about__desc">
          A modern, dark, Linux-native download manager with IDM-class power:
          segmented acceleration, queues, scheduler, categories, site rules,
          grabber, and browser integration.
        </p>
        <div className="about__meta">Built with Rust + Tauri + React.</div>
      </div>
    </Dialog>
  );
}
