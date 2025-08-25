#!/bin/bash

# Test script to debug Redis connectivity issues
# Run this before starting your application

echo "🔍 Testing Redis connectivity..."

# Check if Redis URL is set
REDIS_URL=${REDIS_URL:-"redis://localhost:6379"}
REDIS_PERSECOND_URL=${REDIS_PERSECOND_URL:-""}

echo "📍 Primary Redis URL: $REDIS_URL"
if [ -n "$REDIS_PERSECOND_URL" ]; then
    echo "📍 Per-second Redis URL: $REDIS_PERSECOND_URL"
fi

# Extract host and port from Redis URL
extract_host_port() {
    local url=$1
    # Remove redis:// prefix
    url=${url#redis://}
    # Remove any auth info (user:pass@)
    url=${url#*@}
    # Split host:port
    echo $url
}

test_redis_connection() {
    local url=$1
    local name=$2
    
    echo ""
    echo "🧪 Testing $name Redis connection..."
    
    local host_port=$(extract_host_port $url)
    local host=${host_port%:*}
    local port=${host_port#*:}
    
    # Default port if not specified
    if [ "$host" = "$port" ]; then
        port=6379
    fi
    
    echo "   Host: $host"
    echo "   Port: $port"
    
    # Test network connectivity
    echo "   Testing network connectivity..."
    if timeout 5 nc -z "$host" "$port" 2>/dev/null; then
        echo "   ✅ Network connection successful"
    else
        echo "   ❌ Network connection failed"
        echo "   💡 Check if Redis server is running and accessible"
        return 1
    fi
    
    # Test Redis ping if redis-cli is available
    if command -v redis-cli &> /dev/null; then
        echo "   Testing Redis PING..."
        if timeout 5 redis-cli -h "$host" -p "$port" ping 2>/dev/null | grep -q PONG; then
            echo "   ✅ Redis PING successful"
        else
            echo "   ❌ Redis PING failed"
            echo "   💡 Redis server may not be responding"
            return 1
        fi
    else
        echo "   ⚠️  redis-cli not available, skipping PING test"
    fi
    
    return 0
}

# Test primary Redis
test_redis_connection "$REDIS_URL" "Primary"
primary_status=$?

# Test per-second Redis if configured
persecond_status=0
if [ -n "$REDIS_PERSECOND_URL" ]; then
    test_redis_connection "$REDIS_PERSECOND_URL" "Per-second"
    persecond_status=$?
fi

echo ""
echo "📊 Summary:"
if [ $primary_status -eq 0 ]; then
    echo "   ✅ Primary Redis: OK"
else
    echo "   ❌ Primary Redis: FAILED"
fi

if [ -n "$REDIS_PERSECOND_URL" ]; then
    if [ $persecond_status -eq 0 ]; then
        echo "   ✅ Per-second Redis: OK"
    else
        echo "   ❌ Per-second Redis: FAILED"
    fi
fi

if [ $primary_status -eq 0 ] && [ $persecond_status -eq 0 ]; then
    echo ""
    echo "🎉 All Redis connections are working!"
    echo "Your application should be able to connect successfully."
    exit 0
else
    echo ""
    echo "⚠️  Some Redis connections failed."
    echo "Please fix the connectivity issues before starting your application."
    exit 1
fi
