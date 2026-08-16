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

    # Check for agy CLI binary processes
    case "$line" in
        *"agy"*)
            case "$line" in
                *"get-antigravity-usage"*|*"grep"*) ;;
                *)
                    pid=$(printf '%s\n' "$line" | awk '{print $2}')
                    case "$pid" in
                        ''|*[!0-9]*) continue ;;
                    esac

                    case " $SEEN_PIDS " in
                        *" $pid "*) continue ;;
                    esac

                    cmdline=$(printf '%s\n' "$line" | awk '{for(i=11;i<=NF;i++) printf "%s ", $i; print ""}')
                    is_agy=0
                    case "$cmdline" in
                        "agy "*|"agy") is_agy=1 ;;
                        *"/bin/agy"*|*"/agy "*) is_agy=1 ;;
                    esac

                    if [ "$is_agy" = "1" ]; then
                        SEEN_PIDS="$SEEN_PIDS $pid"
                        PROCESSES_FOUND=$((PROCESSES_FOUND + 1))
                        proc_type="cli"

                        # Extract --csrf_token and --extension_server_port to avoid 401s on agy CLI Connect RPC calls.
                        csrf_token=$(extract_arg "$cmdline" "--csrf_token")
                        ext_port=$(extract_arg "$cmdline" "--extension_server_port")
                        hdr_csrf=""
                        if [ -n "$csrf_token" ]; then
                            hdr_csrf="-H X-Codeium-Csrf-Token:$csrf_token"
                        fi
                        _log "matched agy CLI process line (PID $pid, csrf_token=$([ -n "$csrf_token" ] && echo present || echo absent), ext_port=${ext_port:-none})"

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
                            _log "no listening ports found for CLI PID $pid"
                            continue
                        fi

                        found_base_url=""

                        # Probe RetrieveUserQuotaSummary directly to bypass 404 on root `/` on new builds.
                        RPC_PATH="/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary"
                        RPC_BODY='{"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en"}}'

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
                                _log "CLI port $p RPC probe: status $code via HTTPS"
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
                                    _log "CLI port $p RPC probe: status $code via HTTP"
                                    break
                                fi
                            done
                        fi

                        if [ -z "$found_base_url" ]; then
                            _log "could not connect to RPC on CLI PID $pid — trying next process"
                            continue
                        fi

                        status_resp=$(curl -sk -w "\n%{http_code}" --max-time 2 -X POST \
                            -H "Accept: application/json" \
                            -H "Content-Type: application/json" \
                            -H "Connect-Protocol-Version: 1" \
                            $hdr_csrf \
                            -d '{"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en"}}' \
                            "${found_base_url}/exa.language_server_pb.LanguageServerService/GetUserStatus" 2>/dev/null || printf '\n000')

                        status_code=$(printf '%s\n' "$status_resp" | tail -n 1)
                        status_body=$(printf '%s\n' "$status_resp" | sed '$d')

                        # Log 401 without --csrf_token to distinguish permanent token requirements from transient restarts.
                        if [ "$status_code" = "401" ] && [ -z "$csrf_token" ]; then
                            _log "CLI PID $pid: GetUserStatus returned 401 and this process exposes no --csrf_token argv - its quota cannot be read"
                        fi

                        summary_resp=$(curl -sk -w "\n%{http_code}" --max-time 2 -X POST \
                            -H "Accept: application/json" \
                            -H "Content-Type: application/json" \
                            -H "Connect-Protocol-Version: 1" \
                            $hdr_csrf \
                            -d '{"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en"}}' \
                            "${found_base_url}/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary" 2>/dev/null || printf '\n000')

                        summary_code=$(printf '%s\n' "$summary_resp" | tail -n 1)
                        summary_body=$(printf '%s\n' "$summary_resp" | sed '$d')

                        printf '|||AGPROC|||%s\n' "$pid"
                        printf '|||TYPE|||%s\n' "$proc_type"
                        printf '|||STATUSCODE|||%s\n' "$status_code"
                        printf '|||STATUS|||%s\n' "$status_body"
                        printf '|||SUMMARYCODE|||%s\n' "${summary_code:-0}"
                        printf '|||SUMMARY|||%s\n' "$summary_body"

                        FRAMES_EMITTED=$((FRAMES_EMITTED + 1))
                    fi
                    ;;
            esac
            ;;
    esac

done <<EOF_PS
$PS_OUT
EOF_PS

if [ "$PROCESSES_FOUND" = "0" ]; then
    printf '{"error":"Antigravity IDE or CLI process is not running."}\n' >&2
    exit 1
fi

if [ "$FRAMES_EMITTED" = "0" ]; then
    printf '{"error":"Could not fetch user status from any running Antigravity process."}\n' >&2
    exit 1
fi

exit 0
