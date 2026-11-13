// =============================================================================
// useSuggestionCardNodes — one DOM node per review suggestion card
// =============================================================================

import { useCallback, useRef } from "react";

/** The DOM node a suggestion's card lives in (`useSuggestionCardNodes`). */
export type SuggestionCardNodes = (suggestionId: string) => HTMLElement;

/**
 * One DOM node per suggestion, for as long as the editor is mounted. Cards
 * are rendered into these nodes by `SuggestionCardPortals`, and the places a
 * card is shown (`SuggestionCardSlot`) adopt its node. A card whose place
 * changes (its block collapses or is filtered out, its line moves) keeps its
 * component and with it what is typed in it, a send in flight and its error;
 * only its node moves (`g5.documents.comments.failure`). Monaco's view zones
 * keep their cards the same way (`MonacoSuggestionZones`).
 */
export function useSuggestionCardNodes(): SuggestionCardNodes {
  const nodes = useRef(new Map<string, HTMLElement>());
  return useCallback((suggestionId: string) => {
    let node = nodes.current.get(suggestionId);
    if (!node) {
      node = document.createElement("div");
      node.dataset.testid = `suggestion-card-node-${suggestionId}`;
      nodes.current.set(suggestionId, node);
    }
    return node;
  }, []);
}
