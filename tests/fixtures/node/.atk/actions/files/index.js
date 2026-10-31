// Writes step files the way @actions/core's issueFileCommand does: it throws when the file
// doesn't exist instead of creating it.
const fs = require("fs");
const os = require("os");
const path = require("path");

function issueFileCommand(command, message) {
  const file = process.env[`GITHUB_${command}`];
  if (!file) throw new Error(`Unable to find environment variable for file command ${command}`);
  if (!fs.existsSync(file)) throw new Error(`Missing file at path: ${file}`);
  fs.appendFileSync(file, `${message}${os.EOL}`, { encoding: "utf8" });
}

const keyValue = (key, value) => `${key}<<ghadelimiter${os.EOL}${value}${os.EOL}ghadelimiter`;

const bin = path.join(process.env.RUNNER_TEMP, "node-bin");
fs.mkdirSync(bin, { recursive: true });
fs.writeFileSync(path.join(bin, "node-tool"), "#!/bin/sh\necho node-tool is on PATH\n", {
  mode: 0o755,
});

issueFileCommand("PATH", bin);
issueFileCommand("OUTPUT", keyValue("greeting", "hello from node"));
issueFileCommand("STATE", keyValue("saved", "yes"));
issueFileCommand("STEP_SUMMARY", "## Done");
