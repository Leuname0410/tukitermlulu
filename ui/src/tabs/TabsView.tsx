import type { JSX } from "react";
import { SplitPane } from "../layout/SplitPane";
import { SftpSidebar } from "../sftp/SftpSidebar";
import { TerminalView } from "../terminal/TerminalView";
import type { TabController } from "./tabController";
import type { Tab } from "./tabState";
import { TabPrompt } from "./TabPrompt";

interface TabsViewProps {
  tabs: readonly Tab[];
  activeTabId: number | null;
  onSelect: (id: number) => void;
  controllers: ReadonlyMap<number, TabController>;
  onPromptCancelled: (id: number) => void;
  onUpdateSavedPassword: (sessionId: number, value: string) => void;
}

function statusLabel(tab: Tab): string {
  switch (tab.status.kind) {
    case "connecting":
      return "Conectando…";
    case "connected":
      return "Conectado";
    case "dropped":
      return "Desconectado";
    case "error":
      return "Error";
    case "needs_password":
    case "needs_passphrase":
    case "needs_host_key_approval":
      return "Acción requerida";
  }
}

export function TabsView({
  tabs,
  activeTabId,
  onSelect,
  controllers,
  onPromptCancelled,
  onUpdateSavedPassword,
}: TabsViewProps): JSX.Element {
  return (
    <div>
      <div role="tablist" aria-label="Pestañas">
        {tabs.map((tab) => {
          const selected = tab.id === activeTabId;
          const controller = controllers.get(tab.id);
          return (
            <div key={tab.id}>
              <button
                type="button"
                role="tab"
                aria-selected={selected}
                aria-controls={`tabpanel-${tab.id}`}
                id={`tab-${tab.id}`}
                onClick={() => onSelect(tab.id)}
              >
                {tab.title}
                <span> {statusLabel(tab)}</span>
              </button>
              <button
                type="button"
                aria-label={`Cerrar ${tab.title}`}
                onClick={() => {
                  void controller?.close();
                }}
              >
                Cerrar
              </button>
            </div>
          );
        })}
      </div>
      {tabs.map((tab) => {
        const selected = tab.id === activeTabId;
        const controller = controllers.get(tab.id);
        return (
          <div
            key={tab.id}
            role="tabpanel"
            id={`tabpanel-${tab.id}`}
            aria-labelledby={`tab-${tab.id}`}
            hidden={!selected}
          >
            {tab.status.kind === "connected" && controller ? (
              <SplitPane
                sessionId={tab.target.sessionId}
                left={
                  <SftpSidebar connectionId={tab.status.connectionId} followPath={tab.cwd} />
                }
                right={
                  <section aria-label="Terminal">
                    <TerminalView channelId={tab.status.channelId} source={controller} />
                  </section>
                }
              />
            ) : controller ? (
              <TabPrompt
                tab={tab}
                controller={controller}
                onPromptCancelled={onPromptCancelled}
                onUpdateSavedPassword={onUpdateSavedPassword}
              />
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
