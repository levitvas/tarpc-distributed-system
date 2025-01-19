#!/bin/bash

# Hardcoded list of IP:port combinations
declare -a servers=("127.0.0.1:2010" "127.0.0.1:2020" "127.0.0.1:2030")

# Function to send a GET request
send_get_request() {
    local url=$1
    curl -X GET "$url"
}

# Function to send a POST request
send_post_request() {
    local url=$1
    local json=$2
    curl -X POST -H "Content-Type: application/json" -d "$json" "$url"
}

increment_port() {
    local address=$1
    local ip=${address%:*}
    local port=${address#*:}
    echo "$ip:$((port + 1))"
}

echo "Enter the request type (g for GET /health, p for GET /status, j for join) and server indices:"
echo "Example: g 0 (GET /health from server 0), p 1 (POST /status to server 1), j 0 2 (join server 0 to server 2)"
# Main script logic
while true; do
    echo "Available servers:"
    for i in "${!servers[@]}"; do
        echo "$i: ${servers[$i]}"
    done

    echo "Enter the request type (g/s/j/l) and server indices:"
    read -r request_type arg1 arg2

    case $request_type in
        g)
            # GET /health request
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid index. Please enter a valid server index."
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/health"
            echo "Sending GET request to $url"
            send_get_request "$url"
            ;;
        s)
            # GET /status request
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid index. Please enter a valid server index."
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/status"
            echo "Sending GET request to $url"
            send_get_request "$url"
            ;;
        j)
            # Join request
            if [[ -z "${servers[$arg1]}" || -z "${servers[$arg2]}" ]]; then
                echo "Invalid indices. Please enter valid server indices."
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/joinother"
            json="{\"address\": \"${servers[$arg2]}\"}"  # Original port in payload
            echo "Sending POST request to $url with JSON: $json"
            send_post_request "$url" "$json"
            ;;
        l)
            # POST /leave request
            if [[ -z "${servers[$arg1]}" ]]; then
                echo "Invalid index. Please enter a valid server index."
                continue
            fi
            url="http://$(increment_port "${servers[$arg1]}")/leave"
            echo "Sending POST request to $url"
            send_post_request "$url"
            ;;
        *)
            echo "Invalid request type. Use 'g' for GET /health, 'p' for POST /status, or 'j' for join."
            ;;
    esac
done