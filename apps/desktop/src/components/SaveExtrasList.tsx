import type { ReactNode } from "react";
import { groupSaveItems, type ExtraItem } from "../lib/extras";
import { formatBytes } from "../lib/format";

export function SaveExtrasList({ items, actions }: {
  items: ExtraItem[];
  actions: (item: ExtraItem) => ReactNode;
}) {
  return <div className="extras-save-categories">
    {groupSaveItems(items).map(category => {
      const source = category.items[0];
      const downloadOnly = category.items.every(item => !item.installable);
      return <section className="panel extras-save-category" key={category.key} aria-labelledby={`save-format-${category.key}`}>
        <header className="extras-save-category-heading">
          <h3 id={`save-format-${category.key}`}>{category.name}</h3>
          <span>{category.items.length} {category.items.length === 1 ? "save" : "saves"}{downloadOnly ? " · Somente download" : ""}</span>
        </header>
        {category.regions.map(region => <section className="extras-save-region" key={region.name} aria-label={region.name}>
          <h4>{region.name} <span>{region.items.length}</span></h4>
          <ul className="extras-save-rows">
            {region.items.map(item => <li className="extras-save-row" key={item.id}>
              <div className="extras-save-copy">
                <h5>{item.game_key.startsWith("GFQ:") ? item.title.split(" · ")[0] : item.title}</h5>
                <p className="extras-save-meta">{item.author} · {formatBytes(item.bytes)}</p>
                {item.description && <p className="extras-description">{item.description}</p>}
              </div>
              {actions(item)}
            </li>)}
          </ul>
        </section>)}
        <details className="extras-save-origin">
          <summary>Origem e compatibilidade</summary>
          <a href={source.source_url} target="_blank" rel="noreferrer">{source.source}</a>
          <p>{source.license}</p>
          {[...new Set(category.items.map(item => item.install_note).filter(Boolean))].map(note => <p key={note}>{note}</p>)}
        </details>
      </section>;
    })}
  </div>;
}
