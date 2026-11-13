{{/*
Common labels
*/}}
{{- define "g5.labels" -}}
app.kubernetes.io/name: {{ .Chart.Name }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version }}
{{- end }}

{{/*
Image tag helper — falls back to global.imageTag
*/}}
{{- define "g5.imageTag" -}}
{{- .tag | default $.global.imageTag | default "latest" -}}
{{- end }}

{{/*
Full image reference helper
Usage: {{ include "g5.image" (dict "image" .Values.api.image "global" .Values.global "component" "api") }}
If image.reference is set, uses it directly. Otherwise falls back to
image.repository or builds from global.registry + component name.
*/}}
{{- define "g5.image" -}}
{{- if .image.reference -}}
{{ .image.reference }}
{{- else -}}
  {{- $repo := .image.repository -}}
  {{- if not $repo -}}
    {{- $repo = printf "%s/%s" .global.registry .component -}}
  {{- end -}}
  {{- $tag := .image.tag | default .global.imageTag | default "latest" -}}
{{ printf "%s:%s" $repo $tag }}
{{- end -}}
{{- end -}}

{{/*
Topology mode helpers
Supported modes:
  - full
  - control-plane
*/}}
{{- define "g5.topologyMode" -}}
{{- .Values.topology.mode | default "full" -}}
{{- end -}}

{{- define "g5.controlPlaneEnabled" -}}
{{- $mode := include "g5.topologyMode" . -}}
{{- if or (eq $mode "full") (eq $mode "control-plane") -}}true{{- end -}}
{{- end -}}

{{/*
Shared g5-api-binary process secret sources (envFrom).
Consumed by BOTH the g5-api (G5_ROLE=api) and g5-daemon (G5_ROLE=daemon)
Deployments so the two processes that run the same binary receive identical
secret projections: the core g5-api Secret plus the dedicated, isolated,
optional platform-issue-reporting, G5 GitHub App and outbound email Secrets. Defined once here so a future
secret-bundle addition cannot silently reach only one role — the split-role
delivery gap that let a submission enqueue on the API while the daemon's
outbox worker stayed fail-closed (helm [D7]/[I10]/[D25]).
Usage: {{ include "g5.apiSecretEnvFrom" . | nindent 12 }} under `envFrom:`.
*/}}
{{- define "g5.apiSecretEnvFrom" -}}
- secretRef:
    name: {{ .Values.api.secrets.secretName }}
- secretRef:
    name: g5-platform-issue-reporting-secrets
    optional: true
- secretRef:
    name: g5-github-app-secrets
    optional: true
- secretRef:
    name: g5-email-secrets
    optional: true
{{- end -}}

{{/*
The repository store's mount: only the tenant prefix's subdirectory of the
Filestore share, at the same path as before (`{mountPath}/{prefix}`), so the
repository configuration is unchanged and nothing else on the share, such as
the attachment store, is reachable through it. Without a prefix the store is
the share root itself.
*/}}
{{- define "g5.repoStorageMount" -}}
{{- if .prefix -}}
- name: repos
  mountPath: {{ printf "%s/%s" (trimSuffix "/" .mountPath) .prefix }}
  subPath: {{ .prefix | quote }}
{{- else -}}
- name: repos
  mountPath: {{ .mountPath }}
{{- end }}
{{- end -}}
