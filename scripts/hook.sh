# Token Pipeline Bash Hook
# Source this file in ~/.bashrc:  source ~/.local/share/token-pipeline/hook.sh
# Or install with: tp init

# Commands that tp can optimize
_TP_COMMANDS="git ls cat bat head tail less more cargo npm pnpm yarn bun pytest jest vitest rspec grep rg ag find fd docker podman kubectl oc helm env printenv curl wget httpie tree ps df make cmake ninja python python3 node ruby php gh"

# Intercept commands before execution
__tp_preexec() {
    # Skip if tp is already being used
    [[ "$1" == "tp "* ]] && return
    [[ "$1" == "rtk "* ]] && return
    [[ "$1" == "\\"* ]] && return  # Escaped command (e.g. \git status)

    local cmd="${1%% *}"

    # Check if command is in our list
    for _c in $_TP_COMMANDS; do
        if [[ "$cmd" == "$_c" ]]; then
            # Only intercept non-piped, non-redirected commands
            # that look like they're for an AI agent (long output intended for context)
            if [[ "$1" != *"|"* ]] && [[ "$1" != *">"* ]]; then
                eval "tp run $1"
                return 2  # Signal to bash to skip original command
            fi
        fi
    done
}

# Install the hook using DEBUG trap (works in bash 4+)
__tp_install_hook() {
    if [[ -z "$_TP_HOOK_INSTALLED" ]]; then
        # Use DEBUG trap for pre-exec interception
        # Only intercept if the shell is non-interactive (AI agent mode)
        if [[ -z "$PS1" ]]; then
            trap '__tp_preexec "$BASH_COMMAND"' DEBUG
        fi
        export _TP_HOOK_INSTALLED=1
    fi
}

__tp_install_hook
