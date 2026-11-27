import { useEffect, useReducer, useRef, useState, type JSX } from "react";
import { QuickConnect } from "./connect/QuickConnect";
import { TabsView } from "./tabs/TabsView";
import { createTabController, type TabController } from "./tabs/tabController";
import { tabsReducer, type ConnectTarget, type Tab } from "./tabs/tabState";

const DEFAULT_SIZE = { rows: 24, cols: 80 };

function tabTitle(target: ConnectTarget): string {
  return `${target.username}@${target.host}`;
}

export default function App(): JSX.Element {
  const [tabs, dispatch] = useReducer(tabsReducer, [] as readonly Tab[]);
  const [activeTabId, setActiveTabId] = useState<number | null>(null);
  const [controllers, setControllers] = useState<ReadonlyMap<number, TabController>>(new Map());
  const controllersRef = useRef<Map<number, TabController>>(new Map());
  const nextIdRef = useRef(1);

  useEffect(() => {
    const liveIds = new Set(tabs.map((t) => t.id));
    let changed = false;
    for (const id of controllersRef.current.keys()) {
      if (!liveIds.has(id)) {
        controllersRef.current.delete(id);
        changed = true;
      }
    }
    if (changed) {
      setControllers(new Map(controllersRef.current));
    }
  }, [tabs]);

  function handleConnect(target: ConnectTarget): void {
    const id = nextIdRef.current;
    nextIdRef.current += 1;
    const tab: Tab = {
      id,
      title: tabTitle(target),
      target,
      status: { kind: "connecting" },
    };
    const controller = createTabController({ id, target }, dispatch);
    controllersRef.current.set(id, controller);
    setControllers(new Map(controllersRef.current));
    dispatch({ type: "opened", tab });
    setActiveTabId(id);
    void controller.connect({}, DEFAULT_SIZE);
  }

  const liveIds = new Set(tabs.map((t) => t.id));
  const effectiveActive =
    activeTabId !== null && liveIds.has(activeTabId) ? activeTabId : (tabs.at(-1)?.id ?? null);

  return (
    <main>
      <QuickConnect onSubmit={handleConnect} />
      <TabsView
        tabs={tabs}
        activeTabId={effectiveActive}
        onSelect={setActiveTabId}
        controllers={controllers}
      />
    </main>
  );
}
