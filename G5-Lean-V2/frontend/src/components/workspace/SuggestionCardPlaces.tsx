// =============================================================================
// Suggestion card places — one card per suggestion, shown where it belongs
// =============================================================================

import React, { useCallback } from "react";
import { createPortal } from "react-dom";
import type { SuggestionCardNodes } from "../../hooks/useSuggestionCardNodes";
import type { ReviewSuggestion } from "../../lib/types";

/** Renders each suggestion's card once, into its node. */
export const SuggestionCardPortals: React.FC<{
  nodes: SuggestionCardNodes;
  suggestions: ReviewSuggestion[];
  renderCard: (suggestion: ReviewSuggestion) => React.ReactNode;
}> = ({ nodes, suggestions, renderCard }) => (
  <>
    {suggestions.map((suggestion) =>
      createPortal(renderCard(suggestion), nodes(suggestion.id), suggestion.id),
    )}
  </>
);

/** Where a suggestion's card is shown: adopts the card's node. */
export const SuggestionCardSlot: React.FC<{
  nodes: SuggestionCardNodes;
  suggestionId: string;
}> = ({ nodes, suggestionId }) => {
  const adopt = useCallback(
    (slot: HTMLDivElement | null) => {
      if (slot) slot.appendChild(nodes(suggestionId));
    },
    [nodes, suggestionId],
  );
  return <div ref={adopt} />;
};
