// =============================================================================
// ComposerOwner — provides the page's composer store
// =============================================================================

import React, { useContext } from "react";
import {
  ComposerStoreContext,
  useScopedComposerStore,
} from "../../hooks/useComposerStore";

/**
 * The owner of what is typed into the composers below, for one scope (a
 * workspace) and principal: another scope, person or org starts empty.
 */
export const ComposerOwner: React.FC<{
  scope: string;
  children: React.ReactNode;
}> = ({ scope, children }) => {
  const store = useScopedComposerStore(scope);
  return (
    <ComposerStoreContext.Provider value={store}>
      {children}
    </ComposerStoreContext.Provider>
  );
};

/** The owner above, or one for the views below when there is none. */
export const ComposerOwnerIfNone: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const page = useContext(ComposerStoreContext);
  const own = useScopedComposerStore("");
  return (
    <ComposerStoreContext.Provider value={page ?? own}>
      {children}
    </ComposerStoreContext.Provider>
  );
};
