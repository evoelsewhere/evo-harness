// Connected apps (docs/features/connected-apps/spec.md): the parts Customize →
// Connectors is built from, Evo Mates' page, the key card and a mate's grants.
import { useConnectedAppsStore } from "@/stores/connected-apps-store";
import { useUiStore } from "@/stores/ui-store";

export { AppsCatalogue, CatalogueSearch, ConnectDialog, ServiceDetail, ServiceTile, serviceLine, statusTone, useConnectedServices } from "./apps-parts";
export { ConnectedAppsPage, UnavailableNotice } from "./connected-apps-page";
export { ComposioNote, KeyCard } from "./key-card";
export { MateConnectedApps } from "./mate-apps";
export { ProvidersPanel } from "./providers-panel";
export { ConnectedAppsDialog, openConnectedApps } from "./apps-dialog";
export { OfficialBadge, OfficialHeader, serverTone, useServer, ZapierCard, ZapierHeader } from "./official-parts";

/** Open Customize → Connectors on a connected-apps view. */
export function openConnectorsAt(view: "apps:browse" | "apps:settings") {
  useConnectedAppsStore.getState().setFocus(view);
  useUiStore.getState().openCustomize("connectors");
}
