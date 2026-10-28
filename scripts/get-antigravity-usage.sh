# @docs docs/arch/usage-antigravity.md

PATH="$PATH:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin"
export PATH

_log() {
    printf '[%s][SHELL:ag-usage] %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$1" >&2
}

# ── 1. Host tool availability check ───────────────────────────────────────
if ! command -v curl >/dev/null 2>&1; then
    _log "curl is not installed or not in PATH"
    exit 3
fi

# Target native binary filenames of the Antigravity Language Server across OS/archs
BINARY_NAMES="language_server language_server_macos_arm language_server_macos_x64 language_server_linux_x64 language_server_linux_arm64 language_server_windows_x64.exe"

# Helper: Extract argument value from command line (case-insensitive flag)
# Supports: --arg=val, --arg="val", --arg='val', --arg val, --arg "val", --arg 'val'
extract_arg() {
    _cmd="$1"
    _arg="$2"
    _val=""
    
    # Try arg=value
    _tmp=$(printf '%s\n' "$_cmd" | sed -n -E "s/.*[[:space:]]?${_arg}=([^[:space:]\"']+|\"[^\"]*\"|'[^']*').*/\1/ip")
    if [ -z "$_tmp" ]; then
        # Try arg value
        _tmp=$(printf '%s\n' "$_cmd" | sed -n -E "s/.*[[:space:]]?${_arg}[[:space:]]+([^[:space:]\"']+|\"[^\"]*\"|'[^']*').*/\1/ip")
    fi
    if [ -n "$_tmp" ]; then
        _val=$(printf '%s' "$_tmp" | sed -E 's/^["'\'']|["'\'']$//g')
    fi
    printf '%s' "$_val"
}

# ── 2. Process detection (ps auxww) ────────────────────────────────────────
UNAME=$(uname)
_log "detecting Antigravity processes on platform: $UNAME"

PS_OUT=$(ps auxww 2>/dev/null || ps aux 2>/dev/null || echo "")
if [ -z "$PS_OUT" ]; then
    _log "ps command produced no output"
    printf '{"error":"Antigravity IDE or CLI process is not running."}\n' >&2
    exit 1
fi

SEEN_PIDS=""
PROCESSES_FOUND=0
AGY_RUNNING=0
FRAMES_EMITTED=0

# Parse ps output line by line via heredoc (preserves variable state in POSIX sh)
while IFS= read -r line; do
    [ -z "$line" ] && continue

    # Check if line matches any target binary
    is_target_binary=0
    for bin_name in $BINARY_NAMES; do
        case "$line" in
            *"$bin_name"*)
                is_target_binary=1
                break
                ;;
        esac
    done

    if [ "$is_target_binary" = "1" ]; then
        case "$line" in
            *"--csrf_token"*|*"--extension_server_port"*)
                pid=$(printf '%s\n' "$line" | awk '{print $2}')
                case "$pid" in
                    ''|*[!0-9]*) continue ;;
                esac

                case " $SEEN_PIDS " in
                    *" $pid "*) continue ;;
                esac
                SEEN_PIDS="$SEEN_PIDS $pid"
                PROCESSES_FOUND=$((PROCESSES_FOUND + 1))

                case "$line" in
                    *"Antigravity.app"*)
                        case "$line" in
                            *"Antigravity IDE.app"*) proc_type="ide" ;;
                            *) proc_type="desktop" ;;
                        esac
                        ;;
                    *) proc_type="ide" ;;
                esac

                cmdline=$(printf '%s\n' "$line" | awk '{for(i=11;i<=NF;i++) printf "%s ", $i; print ""}')
                csrf_token=$(extract_arg "$cmdline" "--csrf_token")
                ext_port=$(extract_arg "$cmdline" "--extension_server_port")

                _log "matched Antigravity process line (PID $pid, type $proc_type)"

                # ── 3. Port Discovery for IDE/Desktop process ──────────────
                ports=""
                case "$ext_port" in
                    ''|*[!0-9]*) ;;
                    *)
                        if [ "$ext_port" -gt 0 ]; then
                            ports="$ext_port $((ext_port + 1))"
                        fi
                        ;;
                esac

                if [ "$UNAME" = "Darwin" ]; then
                    lsof_out=$(lsof -nP -iTCP -sTCP:LISTEN -a -p "$pid" 2>/dev/null || true)
                    if [ -n "$lsof_out" ]; then
                        lsof_ports=$(printf '%s\n' "$lsof_out" | sed -n -E 's/.*:([0-9]+)[[:space:]]+\(LISTEN\).*/\1/p')
                        ports="$ports $lsof_ports"
                    fi
                else
                    ss_out=$(ss -tlnp 2>/dev/null | grep "pid=${pid}," || true)
                    if [ -z "$ss_out" ]; then
                        ss_out=$(netstat -tlnp 2>/dev/null | grep "${pid}/" || true)
                    fi
                    if [ -n "$ss_out" ]; then
                        ss_ports=$(printf '%s\n' "$ss_out" | sed -n -E 's/.*:([0-9]+)[[:space:]].*/\1/p')
                        ports="$ports $ss_ports"
                    fi
                fi

                # Deduplicate and filter valid port ranges (0 < port < 65536)
                clean_ports=""
                for p in $ports; do
                    case "$p" in
                        ''|*[!0-9]*) continue ;;
                    esac
                    if [ "$p" -gt 0 ] && [ "$p" -lt 65536 ]; then
                        case " $clean_ports " in
                            *" $p "*) ;;
                            *) clean_ports="$clean_ports $p" ;;
                        esac
                    fi
                done

                if [ -z "$clean_ports" ]; then
                    _log "no listening ports found for PID $pid"
                    continue
                fi

                # ── 4. Connect RPC Probe ────────────────────────────────────
                # Probe RetrieveUserQuotaSummary directly; root `/` returns 404 on new Antigravity IDE builds.
                found_base_url=""
                hdr_csrf=""
                if [ -n "$csrf_token" ]; then
                    hdr_csrf="-H X-Codeium-Csrf-Token:$csrf_token"
                fi

                RPC_PATH="/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary"
                RPC_BODY='{"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en"}}'

                # Try HTTPS first across all candidate ports, then HTTP.
                for p in $clean_ports; do
                    code=$(curl -sk --max-time 1 -o /dev/null -w "%{http_code}" -X POST \
                        -H "Accept: application/json" \
                        -H "Content-Type: application/json" \
                        -H "Connect-Protocol-Version: 1" \
                        $hdr_csrf \
                        -d "$RPC_BODY" \
                        "https://127.0.0.1:$p${RPC_PATH}" 2>/dev/null || echo "000")

                    if [ "$code" = "200" ] || [ "$code" = "401" ]; then
                        found_base_url="https://127.0.0.1:$p"
                        _log "port $p RPC probe: status $code via HTTPS"
                        break
                    fi
                done

                if [ -z "$found_base_url" ]; then
                    for p in $clean_ports; do
                        code=$(curl -sk --max-time 1 -o /dev/null -w "%{http_code}" -X POST \
                            -H "Accept: application/json" \
                            -H "Content-Type: application/json" \
                            -H "Connect-Protocol-Version: 1" \
                            $hdr_csrf \
                            -d "$RPC_BODY" \
                            "http://127.0.0.1:$p${RPC_PATH}" 2>/dev/null || echo "000")

                        if [ "$code" = "200" ] || [ "$code" = "401" ]; then
                            found_base_url="http://127.0.0.1:$p"
                            _log "port $p RPC probe: status $code via HTTP"
                            break
                        fi
                    done
                fi

                if [ -z "$found_base_url" ]; then
                    _log "could not connect to RPC on PID $pid — trying next process"
                    continue
                fi

                # ── 5. RPC Fetch (GetUserStatus & RetrieveUserQuotaSummary) ─
                status_resp=$(curl -sk -w "\n%{http_code}" --max-time 2 -X POST \
                    -H "Accept: application/json" \
                    -H "Content-Type: application/json" \
                    -H "Connect-Protocol-Version: 1" \
                    $hdr_csrf \
                    -d '{"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en"}}' \
                    "${found_base_url}/exa.language_server_pb.LanguageServerService/GetUserStatus" 2>/dev/null || printf '\n000')

                status_code=$(printf '%s\n' "$status_resp" | tail -n 1)
                status_body=$(printf '%s\n' "$status_resp" | sed '$d')

                summary_resp=$(curl -sk -w "\n%{http_code}" --max-time 2 -X POST \
                    -H "Accept: application/json" \
                    -H "Content-Type: application/json" \
                    -H "Connect-Protocol-Version: 1" \
                    $hdr_csrf \
                    -d '{"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en"}}' \
                    "${found_base_url}/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary" 2>/dev/null || printf '\n000')

                summary_code=$(printf '%s\n' "$summary_resp" | tail -n 1)
                summary_body=$(printf '%s\n' "$summary_resp" | sed '$d')

                # Emit frame delimiter to stdout for Rust payload assembler
                printf '|||AGPROC|||%s\n' "$pid"
                printf '|||TYPE|||%s\n' "$proc_type"
                printf '|||STATUSCODE|||%s\n' "$status_code"
                printf '|||STATUS|||%s\n' "$status_body"
                printf '|||SUMMARYCODE|||%s\n' "${summary_code:-0}"
                printf '|||SUMMARY|||%s\n' "$summary_body"

                FRAMES_EMITTED=$((FRAMES_EMITTED + 1))
                ;;
        esac
        continue
    fi

    # agy CLI sessions are only detected here; their quota is read once below via `agy -p "/usage"`.
    case "$line" in
        *"agy"*)
            case "$line" in
                *"get-antigravity-usage"*|*"grep"*) ;;
                *)
                    cmdline=$(printf '%s\n' "$line" | awk '{for(i=11;i<=NF;i++) printf "%s ", $i; print ""}')
                    case "$cmdline" in
                        "agy "*|*"/bin/agy"*|*"/agy "*)
                            pid=$(printf '%s\n' "$line" | awk '{print $2}')
                            _log "matched agy CLI process (PID $pid)"
                            AGY_RUNNING=1
                            PROCESSES_FOUND=$((PROCESSES_FOUND + 1))
                            ;;
                    esac
                    ;;
            esac
            ;;
    esac

done <<EOF_PS
$PS_OUT
EOF_PS

# ── 6. agy CLI quota (docs/arch/usage-antigravity.md § agy CLI source) ─────
# Headless `/usage`: agy's in-process server has no --csrf_token, so its RPC answers 401; ~4 s / ~200 MB a run, hence the 1-minute cache.
AGY_USAGE_BOUND_SECS=15

resolve_agy() {
    for _c in "$HOME/.local/bin/agy" /opt/homebrew/bin/agy /usr/local/bin/agy; do
        [ -x "$_c" ] && { printf '%s' "$_c"; return 0; }
    done
    command -v agy 2>/dev/null
}

agy_cli_frame() {
    agy_bin=$(resolve_agy)
    if [ -z "$agy_bin" ]; then
        _log "agy process found but no agy binary resolves"
        return 1
    fi
    log_file=$(mktemp "${TMPDIR:-/tmp}/aki-agy-usage.XXXXXX") || return 1
    out_file=$(mktemp "${TMPDIR:-/tmp}/aki-agy-usage-out.XXXXXX") || { rm -f "$log_file"; return 1; }
    # A hung `/usage` must not eat the whole 30 s script cap (and the IDE frames with it); output goes to a file so no agy descendant holds a pipe open.
    (cd / && exec "$agy_bin" -p "/usage" --output-format json --log-file "$log_file" </dev/null >"$out_file" 2>/dev/null) &
    agy_pid=$!
    # A hung agy ignores TERM while it waits on its migrations, hence the KILL.
    (sleep "$AGY_USAGE_BOUND_SECS" && kill "$agy_pid" && sleep 2 && kill -9 "$agy_pid") >/dev/null 2>&1 &
    watchdog_pid=$!
    wait "$agy_pid"
    usage_rc=$?
    kill "$watchdog_pid" 2>/dev/null
    usage_json=$(cat "$out_file")
    rm -f "$out_file"
    # agy prints the account only to its log; `/usage` itself carries no email.
    email=$(sed -n 's/.*authenticated successfully as \([^[:space:]]*\).*/\1/p' "$log_file" | tail -n 1)
    rm -f "$log_file"
    if [ "$usage_rc" != "0" ] || [ -z "$usage_json" ]; then
        _log "agy /usage failed (exit $usage_rc)"
        return 1
    fi
    case "$email" in
        ''|*[!A-Za-z0-9._%+@-]*)
            _log "agy /usage answered but its log named no account email"
            return 1
            ;;
    esac
    # `/usage` has no plan tier; the AGY statusline records it per account (statusline-unified.sh, aki-agquota).
    tier=""
    tier_file="$HOME/.gemini/antigravity-cli/aki-plan-tier/$(printf '%s' "$email" | tr -c 'A-Za-z0-9@._+-' '_')"
    [ -f "$tier_file" ] && IFS= read -r tier < "$tier_file"
    case "$tier" in *[!A-Za-z0-9._+\ -]*) _log "agy plan tier record has unexpected characters - ignored"; tier="" ;; esac
    printf '|||AGPROC|||agy-cli\n'
    printf '|||TYPE|||cli\n'
    printf '|||STATUSCODE|||200\n'
    if [ -n "$tier" ]; then
        printf '|||STATUS|||{"email":"%s","userTier":{"name":"%s"}}\n' "$email" "$tier"
    else
        printf '|||STATUS|||{"email":"%s"}\n' "$email"
    fi
    printf '|||SUMMARYCODE|||200\n'
    printf '|||SUMMARY|||%s\n' "$usage_json"
}

if [ "$AGY_RUNNING" = "1" ]; then
    cli_cache="${TMPDIR:-/tmp}/aki-devsync-agy-usage.$(id -u)"
    if [ -n "$(find "$cli_cache" -mmin -1 2>/dev/null)" ]; then
        _log "agy /usage reused from cache (<1 min old)"
        cat "$cli_cache"
        FRAMES_EMITTED=$((FRAMES_EMITTED + 1))
    elif [ -n "$(find "$cli_cache.failed" -mmin -1 2>/dev/null)" ]; then
        _log "agy /usage skipped: last run failed <1 min ago"
    elif cli_frame=$(agy_cli_frame); then
        (umask 077 && printf '%s\n' "$cli_frame" > "$cli_cache.$$" && mv -f "$cli_cache.$$" "$cli_cache")
        rm -f "$cli_cache.failed"
        printf '%s\n' "$cli_frame"
        FRAMES_EMITTED=$((FRAMES_EMITTED + 1))
    else
        : > "$cli_cache.failed"
    fi
fi

if [ "$PROCESSES_FOUND" = "0" ]; then
    printf '{"error":"Antigravity IDE or CLI process is not running."}\n' >&2
    exit 1
fi

if [ "$FRAMES_EMITTED" = "0" ]; then
    printf '{"error":"Could not fetch user status from any running Antigravity process."}\n' >&2
    exit 1
fi

exit 0
