// Render the actual deployed template with the same text/template engine and
// replace function as GoogleCloudPlatform/cloud-build-notifiers/slack/main.go.
// No credentials, network, Slack calls or third-party modules are used.
// Run: GO111MODULE=off go test -v scripts/slack_notifier_template_test.go
package notifier_test

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"text/template"
)

// Typed fixtures preserve the protobuf's enum String method and optional
// Approval/Result pointers; maps would hide nil-pointer template failures.
type buildStatus int32

func (s buildStatus) String() string {
	return map[buildStatus]string{2: "WORKING", 4: "FAILURE", 5: "INTERNAL_ERROR", 6: "TIMEOUT", 9: "EXPIRED"}[s]
}

type approvalResult struct {
	ApproverAccount string
	Comment         string
}

type buildApproval struct {
	Result *approvalResult
}

type build struct {
	Status        buildStatus
	Approval      *buildApproval
	Substitutions map[string]string
	LogUrl        string
	Id            string
}

type textObject struct {
	Type string `json:"type"`
	Text string `json:"text"`
}

type block struct {
	Type   string       `json:"type"`
	Text   *textObject  `json:"text"`
	Fields []textObject `json:"fields"`
}

func render(t *testing.T, event build) []block {
	t.Helper()
	_, filename, _, _ := runtime.Caller(0)
	path := filepath.Join(filepath.Dir(filename), "../infra/qa/delivery/slack-notifier-config.yaml.tftpl")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	_, body, found := strings.Cut(string(data), "      content: |\n")
	if !found {
		t.Fatal("notifier template content missing")
	}
	body, _, found = strings.Cut(body, "\n  secrets:")
	if !found {
		t.Fatal("notifier template boundary missing")
	}
	tmpl, err := template.New("blockkit_template").Funcs(template.FuncMap{
		"replace": strings.ReplaceAll,
	}).Parse(body)
	if err != nil {
		t.Fatal(err)
	}
	var rendered bytes.Buffer
	if err := tmpl.Execute(&rendered, struct{ Build *build }{&event}); err != nil {
		t.Fatal(err)
	}
	var blocks []block
	if err := json.Unmarshal(rendered.Bytes(), &blocks); err != nil {
		t.Fatalf("invalid rendered Block Kit array: %v\n%s", err, rendered.String())
	}
	if len(blocks) != 2 || blocks[0].Type != "header" || blocks[0].Text == nil || blocks[1].Type != "section" {
		t.Fatalf("unexpected Block Kit structure: %+v", blocks)
	}
	return blocks
}

func event(status buildStatus) build {
	return build{
		Status: status,
		Substitutions: map[string]string{
			"TRIGGER_NAME": "g5-promote-staging",
			"COMMIT_SHA":   "0123456789abcdef",
			"BRANCH_NAME":  "main",
		},
		LogUrl: "https://console.cloud.google.com/cloud-build/builds/test-build",
		Id:     "test-build",
	}
}

func field(t *testing.T, blocks []block, name string) string {
	t.Helper()
	for _, item := range blocks[1].Fields {
		if value, ok := strings.CutPrefix(item.Text, "*"+name+":*\n"); ok {
			return value
		}
	}
	t.Fatalf("missing %s field: %+v", name, blocks[1].Fields)
	return ""
}

func TestApprovalAttribution(t *testing.T) {
	for _, tc := range []struct {
		name     string
		approval *buildApproval
		want     string
	}{
		{"human", &buildApproval{&approvalResult{"kai@example.com", "human comment"}}, "kai@example.com"},
		{"train account", &buildApproval{&approvalResult{"g5-release-train@project.iam.gserviceaccount.com", "Auto-approved by g5-release-train"}}, "g5-release-train@project.iam.gserviceaccount.com"},
		{"train comment", &buildApproval{&approvalResult{"", "Auto-approved by the g5-release-train weekly release train (g5-release-train service account)"}}, "Auto-approved by the g5-release-train weekly release train (g5-release-train service account)"},
		{"absent approval", nil, "unknown"},
		{"absent result", &buildApproval{}, "unknown"},
		{"empty result", &buildApproval{&approvalResult{}}, "unknown"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			build := event(2)
			build.Approval = tc.approval
			blocks := render(t, build)
			if got := blocks[0].Text.Text; got != ":rocket: Release job approved & running" {
				t.Fatalf("unexpected approval header: %q", got)
			}
			if got := field(t, blocks, "Approved by"); got != tc.want {
				t.Fatalf("approver = %q, want %q", got, tc.want)
			}
			if got := field(t, blocks, "Status"); got != "WORKING" {
				t.Fatalf("status = %q", got)
			}
		})
	}
}

func TestAllFailureMessages(t *testing.T) {
	for _, status := range []buildStatus{4, 5, 6, 9} {
		t.Run(status.String(), func(t *testing.T) {
			blocks := render(t, event(status))
			if got := blocks[0].Text.Text; got != ":x: Cloud Build failed: g5-promote-staging" {
				t.Fatalf("unexpected failure header: %q", got)
			}
			if got := field(t, blocks, "Status"); got != status.String() {
				t.Fatalf("status = %q, want %q", got, status.String())
			}
			for _, item := range blocks[1].Fields {
				if strings.Contains(item.Text, "Approved by") {
					t.Fatal("failure must not imply a new approval")
				}
			}
		})
	}
}

func TestEventStringsRemainValidJSON(t *testing.T) {
	value := "Kai's \"release\" \\ path \\' \\u0027\n\t\r\b\f\x00\x1f <>&= \u4e2d\u6587 \U0001f680\u2028"
	for _, status := range []buildStatus{2, 4} {
		for _, useAccount := range []bool{false, true} {
			build := event(status)
			build.Approval = &buildApproval{&approvalResult{Comment: value}}
			if useAccount {
				build.Approval.Result.ApproverAccount = value
			}
			for key := range build.Substitutions {
				build.Substitutions[key] = value
			}
			build.LogUrl, build.Id = value, value
			blocks := render(t, build)
			for _, name := range []string{"Trigger", "Commit", "Branch"} {
				if got := field(t, blocks, name); got != value {
					t.Fatalf("%s = %q, want %q", name, got, value)
				}
			}
			if got := field(t, blocks, "Build"); got != "<"+value+"|"+value+">" {
				t.Fatalf("build link = %q", got)
			}
			if status == 2 {
				if got := field(t, blocks, "Approved by"); got != value {
					t.Fatalf("approver = %q, want %q", got, value)
				}
			} else if got := blocks[0].Text.Text; got != ":x: Cloud Build failed: "+value {
				t.Fatalf("failure header = %q", got)
			}
		}
	}
}
