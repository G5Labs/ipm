-- Labels for G5's native issue tracker (g5.issue.labels.native, issue #1055).
-- A native domain keeps its own label catalog; issues carry labels from it.
-- Assignments reference both the issue and the label through
-- (org_id, domain_id, id), so a label can never be attached to an issue of
-- another organization or domain. A label's stable id keeps its assignments
-- through a rename; issues read the catalog's current name and colour.
-- Label history uses the existing event stream: events name the label as it
-- was called at the time, so a later rename or deletion keeps the history.
SET LOCAL lock_timeout = '5s';

ALTER TABLE public.native_issues
    ADD CONSTRAINT native_issues_org_domain_id_key UNIQUE (org_id, domain_id, id);

CREATE TABLE public.native_issue_labels (
    id bigint GENERATED ALWAYS AS IDENTITY,
    org_id uuid NOT NULL,
    domain_id uuid NOT NULL,
    name text NOT NULL,
    -- Six lowercase hex digits without `#`, or none.
    color text,
    description text DEFAULT '' NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT native_issue_labels_pkey PRIMARY KEY (id),
    CONSTRAINT native_issue_labels_org_domain_id_key UNIQUE (org_id, domain_id, id),
    CONSTRAINT native_issue_labels_name_check CHECK (
        name = btrim(name) AND name <> '' AND char_length(name) <= 50
        AND name !~ '[\r\n]'
    ),
    CONSTRAINT native_issue_labels_color_check CHECK (color ~ '^[0-9a-f]{6}$'),
    CONSTRAINT native_issue_labels_description_check
        CHECK (char_length(description) <= 100 AND description !~ '[\r\n]'),
    CONSTRAINT native_issue_labels_org_fk
        FOREIGN KEY (org_id) REFERENCES public.orgs(id) ON DELETE CASCADE,
    -- A native domain can be removed only without issues; its catalog goes
    -- with it.
    CONSTRAINT native_issue_labels_domain_fk
        FOREIGN KEY (org_id, domain_id) REFERENCES public.issue_domains(org_id, id)
        ON DELETE CASCADE
);
-- Names are unique within a domain without regard to case.
CREATE UNIQUE INDEX native_issue_labels_domain_name
    ON public.native_issue_labels (domain_id, lower(name));

CREATE TABLE public.native_issue_label_assignments (
    org_id uuid NOT NULL,
    domain_id uuid NOT NULL,
    issue_id bigint NOT NULL,
    label_id bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT native_issue_label_assignments_pkey PRIMARY KEY (issue_id, label_id),
    CONSTRAINT native_issue_label_assignments_issue_fk
        FOREIGN KEY (org_id, domain_id, issue_id)
        REFERENCES public.native_issues(org_id, domain_id, id) ON DELETE CASCADE,
    CONSTRAINT native_issue_label_assignments_label_fk
        FOREIGN KEY (org_id, domain_id, label_id)
        REFERENCES public.native_issue_labels(org_id, domain_id, id) ON DELETE CASCADE
);
CREATE INDEX native_issue_label_assignments_label
    ON public.native_issue_label_assignments (label_id);

-- `labeled`/`unlabeled` events name the label as it was then.
ALTER TABLE public.native_issue_events ADD COLUMN label_name text;
ALTER TABLE public.native_issue_events DROP CONSTRAINT native_issue_events_kind_check;
ALTER TABLE public.native_issue_events ADD CONSTRAINT native_issue_events_kind_check
    CHECK (kind IN ('closed', 'reopened', 'assigned', 'unassigned', 'labeled', 'unlabeled'));
ALTER TABLE public.native_issue_events ADD CONSTRAINT native_issue_events_label_check
    CHECK ((kind IN ('labeled', 'unlabeled')) = (label_name IS NOT NULL));

CREATE TRIGGER trg_native_issue_labels_set_org
    BEFORE INSERT ON public.native_issue_labels
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();
CREATE TRIGGER trg_native_issue_label_assignments_set_org
    BEFORE INSERT ON public.native_issue_label_assignments
    FOR EACH ROW EXECUTE FUNCTION public.set_org_id_from_session();

ALTER TABLE public.native_issue_labels ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.native_issue_labels FORCE ROW LEVEL SECURITY;
ALTER TABLE public.native_issue_label_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE ONLY public.native_issue_label_assignments FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.native_issue_labels
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));
CREATE POLICY tenant_isolation ON public.native_issue_label_assignments
    TO g5_tenant
    USING (org_id = ANY (authz.current_org_ids()))
    WITH CHECK (org_id = ANY (authz.current_org_ids()));

-- Catalog labels are edited and deleted (UPDATE also lets a change lock the
-- labels it names); assignments are added and removed.
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.native_issue_labels TO g5_tenant;
GRANT SELECT, INSERT, DELETE ON TABLE public.native_issue_label_assignments TO g5_tenant;
