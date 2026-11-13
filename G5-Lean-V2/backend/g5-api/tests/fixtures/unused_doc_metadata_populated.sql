-- Schema 091 fixture for the restricted-migrator upgrade rehearsal.
-- Keep substantive document fields and discussions in two tenants, including
-- equal paths in separate repositories and both populated/null retired fields.
INSERT INTO orgs(id,name,slug) VALUES
('62000000-0000-4000-8000-000000000001','Metadata 0','metadata-0'),
('62000000-0000-4000-8000-000000000101','Metadata 1','metadata-1');
INSERT INTO users(id,email,display_name) VALUES
('62000000-0000-4000-8000-000000000002','metadata-0@test.invalid','Owner 0'),
('62000000-0000-4000-8000-000000000102','metadata-1@test.invalid','Owner 1');
INSERT INTO org_memberships(org_id,user_id,role) VALUES
('62000000-0000-4000-8000-000000000001','62000000-0000-4000-8000-000000000002','owner'),
('62000000-0000-4000-8000-000000000101','62000000-0000-4000-8000-000000000102','owner');
INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by) VALUES
('62000000-0000-4000-8000-000000000003','62000000-0000-4000-8000-000000000001','metadata-0','62000000-0000-4000-8000-000000000002','62000000-0000-4000-8000-000000000002'),
('62000000-0000-4000-8000-000000000103','62000000-0000-4000-8000-000000000101','metadata-1','62000000-0000-4000-8000-000000000102','62000000-0000-4000-8000-000000000102');
INSERT INTO repos(id,org_id,name,slug,git_url) VALUES
('62000000-0000-4000-8000-000000000010','62000000-0000-4000-8000-000000000001','Docs 0','docs-0','https://example.invalid/0.git'),
('62000000-0000-4000-8000-000000000110','62000000-0000-4000-8000-000000000101','Docs 1','docs-1','https://example.invalid/1.git');
INSERT INTO workspace_repos(org_id,workspace_id,repo_id,root_path,role) VALUES
('62000000-0000-4000-8000-000000000001','62000000-0000-4000-8000-000000000003','62000000-0000-4000-8000-000000000010','docs','docs'),
('62000000-0000-4000-8000-000000000101','62000000-0000-4000-8000-000000000103','62000000-0000-4000-8000-000000000110','docs','docs');

INSERT INTO docs(org_id,workspace_id,repo_id,name,doc_type,file_path,document_source,
                 content,content_hash,linked_doc_paths,last_uplifted_at,last_downlifted_at,
                 content_updated_at,last_uplifted_commit_sha,last_downlifted_commit_sha,
                 created_at,updated_at,persona,instruction,is_future,review_iterations,
                 custom_persona_label,source_files,mapping_content)
SELECT org_id,workspace_id,repo_id,'Retained document','pm_view','views/same.md','user',
       'Retained fallback body',repeat('a',64),'["docs/linked.md"]'::jsonb,
       '2026-07-01','2026-07-02','2026-07-03',repeat('b',40),repeat('c',40),
       '2026-07-01','2026-07-04','pm','Retired view instruction',true,3,
       'Retired label','["src/main.rs"]'::jsonb,'Retired mapping'
FROM workspace_repos;
INSERT INTO docs(org_id,workspace_id,repo_id,name,doc_type,file_path,
                 persona,instruction,is_future,review_iterations,custom_persona_label,
                 source_files,mapping_content)
SELECT org_id,workspace_id,repo_id,'Null metadata','design','docs/null.md',
       NULL,NULL,NULL,NULL,NULL,NULL,NULL
FROM workspace_repos;

INSERT INTO tasks(org_id,workspace_id,name,kind,status,created_by)
SELECT org_id,id,'Retained task','command','queued',owner_user_id FROM workspaces;
INSERT INTO comment_index(org_id,workspace_id,author_id,scope,repo_id,file_path,line_number,
                          content,resolved,resolved_at,origin_branch,created_at,updated_at)
SELECT wr.org_id,wr.workspace_id,w.owner_user_id,scope,wr.repo_id,'views/same.md',7,
       'Retained thread',scope='line',CASE WHEN scope='line' THEN '2026-07-02'::timestamptz END,
       'main','2026-07-01','2026-07-02'
FROM workspace_repos wr JOIN workspaces w ON w.id=wr.workspace_id
CROSS JOIN (VALUES ('line'),('suggestion')) scopes(scope);
INSERT INTO comment_replies(org_id,comment_id,author_id,content,created_at)
SELECT org_id,id,author_id,'Retained reply','2026-07-03' FROM comment_index;
INSERT INTO comment_mentions(workspace_id,comment_id,mentioned_user_id,mentioned_by_user_id,org_id,created_at)
SELECT workspace_id,id,author_id,author_id,org_id,'2026-07-04' FROM comment_index;
