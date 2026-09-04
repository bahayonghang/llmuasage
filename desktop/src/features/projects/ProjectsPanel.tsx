import type { Copy } from "../../app/i18n";
import type { ProjectBreakdown } from "../../app/types";

export function ProjectsPanel({
  projects,
  copy,
  onProjectClick,
}: {
  projects: ProjectBreakdown[];
  copy: Copy;
  onProjectClick: (projectHash: string) => void;
}) {
  return (
    <section id="projects" className="block" data-testid="projects-panel">
      <div className="section-eyebrow">{copy.navProjects}</div>
      <h2 className="section-title">{copy.projectsTitle}</h2>
      {projects.length === 0 ? (
        <p className="muted">{copy.emptyRows}</p>
      ) : (
        <div className="row-list">
          {projects.map((row) => (
            <button
              type="button"
              className="project-row"
              key={row.project_hash}
              data-testid={`project-row-${row.project_hash}`}
              onClick={() => onProjectClick(row.project_hash)}
            >
              <div>{row.project_label || row.project_hash}</div>
              <div className="muted">{row.project_ref || row.project_hash}</div>
              <div className="num">{row.total_tokens}</div>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
