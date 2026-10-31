set -e
echo "docker action says $INPUT_GREETING to $1"
echo "greeting=$INPUT_GREETING" >> "$GITHUB_OUTPUT"
touch "$GITHUB_WORKSPACE/made-by-action"
