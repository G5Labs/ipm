import { memo, useMemo, type ReactNode } from "react";
import { ChevronRight, File, Folder, GitBranch } from "lucide-react";
import type { SourcePatch } from "../../lib/sourceControlSelection";
interface Entry {
  name: string;
  children: Map<string, Entry>;
  key?: string;
}
interface FileTreeProps {
  files: Map<string, SourceControlFileEntry>;
  selected: string | null;
  onSelect: (key: string) => void;
}
export interface SourceControlFileEntry {
  repoId: string;
  repoName: string;
  path: string;
  change: SourcePatch["file"]["change"];
  patchCount: number;
}
/** Only selected changed paths are admitted; no repository browsing request. */
export const SourceControlFileTree = memo(
  function SourceControlFileTree({ files, selected, onSelect }: FileTreeProps) {
    const repos = useMemo(() => {
      const repos = new Map<string, { name: string; root: Entry }>();
      for (const [key, file] of files) {
        let repo = repos.get(file.repoId);
        if (!repo) {
          repo = {
            name: file.repoName,
            root: { name: "", children: new Map() },
          };
          repos.set(file.repoId, repo);
        }
        let parent = repo.root;
        for (const name of file.path.split("/")) {
          let entry = parent.children.get(name);
          if (!entry) {
            entry = { name, children: new Map() };
            parent.children.set(name, entry);
          }
          parent = entry;
        }
        parent.key = key;
      }
      return repos;
    }, [files]);
    function render(entry: Entry): ReactNode {
      return (
        <ul className="pl-3">
          {[...entry.children.values()]
            .sort(
              (a, b) =>
                Number(!a.children.size) - Number(!b.children.size) ||
                a.name.localeCompare(b.name),
            )
            .map((child) => (
              <li key={child.name}>
                {child.key && (
                  <button
                    data-testid="source-control-file"
                    aria-label={child.name}
                    className="source-control-tree-row"
                    title={`${files.get(child.key)!.path} ${
                      files.get(child.key)!.patchCount > 1
                        ? "changed in multiple commits"
                        : files.get(child.key)!.change
                    }`}
                    aria-current={selected === child.key || undefined}
                    onClick={() => onSelect(child.key!)}
                  >
                    <File size={13} aria-hidden="true" />
                    <span>{child.name}</span>
                    <code>
                      {files.get(child.key)!.patchCount > 1
                        ? "±"
                        : ({
                            added: "A",
                            modified: "M",
                            deleted: "D",
                            renamed: "R",
                          }[files.get(child.key)!.change] ?? "M")}
                    </code>
                  </button>
                )}
                {child.children.size > 0 && (
                  <details open className="source-control-tree-folder">
                    <summary className="source-control-tree-row">
                      <ChevronRight size={12} aria-hidden="true" />
                      <Folder size={14} aria-hidden="true" />
                      <span>{child.name}</span>
                    </summary>
                    {render(child)}
                  </details>
                )}
              </li>
            ))}
        </ul>
      );
    }
    return (
      <>
        {[...repos].map(([id, repo]) => (
          <section key={id} aria-label={repo.name}>
            <details open className="source-control-tree-folder">
              <summary className="source-control-tree-row">
                <ChevronRight size={12} aria-hidden="true" />
                <GitBranch size={14} aria-hidden="true" />
                <strong>{repo.name}</strong>
              </summary>
              {render(repo.root)}
            </details>
          </section>
        ))}
      </>
    );
  },
  (before, after) => {
    if (
      before.selected !== after.selected ||
      before.onSelect !== after.onSelect ||
      before.files.size !== after.files.size
    )
      return false;
    if (before.files === after.files) return true;
    const previous = before.files.entries();
    for (const [key, file] of after.files) {
      const entry = previous.next().value;
      if (!entry || entry[0] !== key) return false;
      const old = entry[1];
      if (
        old.repoId !== file.repoId ||
        old.repoName !== file.repoName ||
        old.path !== file.path ||
        old.patchCount !== file.patchCount ||
        old.change !== file.change
      )
        return false;
    }
    return true;
  },
);
