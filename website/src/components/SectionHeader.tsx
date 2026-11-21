type SectionHeaderProps = {
  index: string;
  label: string;
  title: string;
  intro: string;
};

export default function SectionHeader({ index, label, title, intro }: SectionHeaderProps) {
  return (
    <header className="section-head">
      <p className="section-kicker">
        <span className="section-index">[{index}]</span>
        <span>{label}</span>
      </p>
      <div className="section-head-body">
        <h2>{title}</h2>
        <p className="section-intro">{intro}</p>
      </div>
    </header>
  );
}
