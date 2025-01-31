#!/bin/bash

# Configuration
declare -a servers=("127.0.0.1:2010" "127.0.0.1:2020" "127.0.0.1:2030" "127.0.0.1:2040" "127.0.0.1:2050")

# Helper Functions
send_get_request() {
    local url=$1
    curl -X GET "$url"
    echo
}

send_post_request() {
    local url=$1
    local json=$2
    curl -X POST -H "Content-Type: application/json" -d "$json" "$url"
    echo
}

increment_port() {
    local address=$1
    local ip=${address%:*}
    local port=${address#*:}
    echo "$ip:$((port + 1))"
}

print_help() {
    echo "Commands:"
    echo "g <idx>                  - Get health status"
    echo "s <idx>                  - Get node status"
    echo "j <from_idx> <to_idx>    - Join nodes"
    echo "l <idx>                  - Node leaves"
    echo "k <idx>                  - Kill node"
    echo "r <idx>                  - Revive node"
    echo "acq <idx> <resource>     - Acquire resource"
    echo "rel <idx> <resource>     - Release resource"
    echo "det <idx>                - Start detection"
    echo "wait <idx> <target_idx>  - Wait for message"
    echo "active <idx>             - Set active"
    echo "passive <idx>            - Set passive"
    echo "delay <idx> <ms>         - Set delay"
    echo "h                        - Help"
    echo "q                        - Quit"
}

print_servers() {
    echo "Available nodes:"
    for i in "${!servers[@]}"; do
        echo "$i: ${servers[$i]}"
    done
}

# Main Loop
while true; do
    echo -e "\nEnter command (h for help):"
    print_servers
    read -r cmd arg1 arg2

    case $cmd in
        h)
            print_help
            ;;
        q)
            exit 0
            ;;
        g)
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid node index"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/health"
            send_get_request "$url"
            ;;
        s)
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid node index"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/status"
            send_get_request "$url"
            ;;
        j)
            if [[ -z "${servers[$arg1]}" || -z "${servers[$arg2]}" ]]; then
                echo "Invalid node indices"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/joinother"
            json="{\"address\": \"${servers[$arg2]}\"}"
            send_post_request "$url" "$json"
            ;;
        l)
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid node index"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/leave"
            send_post_request "$url"
            ;;
        k)
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid node index"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/kill"
            send_post_request "$url"
            ;;
        r)
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid node index"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/revive"
            send_post_request "$url"
            ;;
        acq)
            if [[ -z "${servers[$arg1]}" || -z "$arg2" ]]; then
                echo "Usage: acq <node_idx> <resource>"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/acquire"
            json="{\"resource\": \"$arg2\"}"
            send_post_request "$url" "$json"
            ;;
        rel)
            if [[ -z "${servers[$arg1]}" || -z "$arg2" ]]; then
                echo "Usage: rel <node_idx> <resource>"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/release"
            json="{\"resource\": \"$arg2\"}"
            send_post_request "$url" "$json"
            ;;
        det)
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid node index"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/detection/start"
            send_post_request "$url"
            ;;
        wait)
            if [[ -z "${servers[$arg1]}" || -z "${servers[$arg2]}" ]]; then
                echo "Invalid node indices"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/waitForMessage"
            json="{\"address\": \"${servers[$arg2]}\"}"
            send_post_request "$url" "$json"
            ;;
        active)
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid node index"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/setActive"
            send_post_request "$url"
            ;;
        passive)
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid node index"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/setPassive"
            send_post_request "$url"
            ;;
        delay)
            if [[ -z "${servers[$arg1]}" || -z "$arg2" ]]; then
                echo "Usage: delay <node_idx> <milliseconds>"
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/delay"
            json="{\"delay_ms\": $arg2}"
            send_post_request "$url" "$json"
            ;;
        *)
            echo "Invalid command. Use 'h' for help."
            ;;
    esac
done