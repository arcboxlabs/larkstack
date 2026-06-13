import { Tabs } from "@base-ui/react/tabs";

export type TabDef = { id: string; label: string };

/// The console's primary navigation, built on Base UI Tabs. Render inside a
/// <Tabs.Root> so each tab can read the selected-value context.
export function TabBar({ tabs }: { tabs: ReadonlyArray<TabDef> }) {
  return (
    <Tabs.List className="tabs">
      {tabs.map((t) => (
        <Tabs.Tab key={t.id} value={t.id} className="tab">
          {t.label}
        </Tabs.Tab>
      ))}
    </Tabs.List>
  );
}
