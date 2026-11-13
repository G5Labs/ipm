"""The flag gate must inspect permission-prompts choices, not unrelated text."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class ClaudeCliFlagsTest(unittest.TestCase):
    def test_none_must_be_a_permission_prompts_choice(self):
        script = Path(__file__).with_name("check-claude-cli-flags.sh")
        flags = [
            "print", "output-format", "input-format", "verbose", "include-partial-messages",
            "resume", "session-id", "permission-mode", "permission-prompt-tool", "mcp-config",
            "strict-mcp-config", "settings", "add-dir", "setting-sources", "disallowedTools",
            "append-system-prompt", "system-prompt", "tools", "allowedTools", "effort", "model",
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cli = root / "claude"
            cli.write_text("""#!/bin/sh
case "$1" in
--version) echo '2.1.285 (Claude Code)' ;;
--help) cat "$CLAUDE_FIXTURE_HELP" ;;
--max-turns) echo 'invalid --max-turns' >&2; exit 1 ;;
esac
""")
            cli.chmod(0o755)
            help_file = root / "help"
            for indent in ("", "  "):
                for valid in (True, False):
                    with self.subTest(indent=indent, valid=valid):
                        help_file.write_text(
                            "\n".join(indent + "--" + flag for flag in flags)
                            + f'\n{indent}--output-format <format> (choices: "stream-json")'
                            + f'\n{indent}--permission-mode <mode> (choices: "bypassPermissions", "dontAsk")'
                            + f'\n{indent}--unrelated <mode> (choices: "none")'
                            + f'\n{indent}--permission-prompts <target> Who answers permission prompts with'
                            + '\n                                        --permission-prompt-tool) or "none"'
                            + '\n                                        (choices: "host"'
                            + (', "none"' if valid else '')
                            + f', default: "host")\n{indent}--next-option\n'
                        )
                        result = subprocess.run(
                            ["sh", str(script), str(cli), "2.1.285"],
                            env={**os.environ, "CLAUDE_FIXTURE_HELP": str(help_file)},
                            capture_output=True, text=True, timeout=15,
                        )
                        self.assertEqual(result.returncode == 0, valid, result.stdout + result.stderr)
                        if not valid:
                            self.assertIn("permission-prompts=none", result.stderr)


if __name__ == "__main__":
    unittest.main()
