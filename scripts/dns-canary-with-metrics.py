#!/usr/bin/env python3
"""
Enhanced DNS Canary Script with Prometheus Metrics Endpoint

This version exposes metrics via HTTP endpoint even when DNS servers are down,
ensuring continuous monitoring data availability.
"""

import random
import time
import subprocess
import sys
import signal
import logging
import requests
import json
import uuid
from typing import List, Tuple, Dict, Optional
from datetime import datetime
from http.server import HTTPServer, BaseHTTPRequestHandler
import threading

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler('dns-canary-metrics.log'),
        logging.StreamHandler(sys.stdout)
    ]
)
logger = logging.getLogger(__name__)

class MetricsHandler(BaseHTTPRequestHandler):
    """HTTP handler for Prometheus metrics endpoint"""
    
    def do_GET(self):
        if self.path == '/metrics':
            self.send_response(200)
            self.send_header('Content-type', 'text/plain; version=0.0.4; charset=utf-8')
            self.end_headers()
            
            # Get metrics from the canary instance
            metrics = self.server.canary.get_prometheus_metrics()
            self.wfile.write(metrics.encode('utf-8'))
        elif self.path == '/health':
            self.send_response(200)
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            health = {"status": "healthy", "timestamp": datetime.utcnow().isoformat()}
            self.wfile.write(json.dumps(health).encode('utf-8'))
        else:
            self.send_response(404)
            self.end_headers()
    
    def log_message(self, format, *args):
        # Suppress HTTP access logs
        pass

class DNSCanaryWithMetrics:
    def __init__(self, dns_server="localhost", dns_port=12312, api_server="localhost", api_port=8080, metrics_port=8090):
        self.dns_server = dns_server
        self.dns_port = dns_port
        self.api_server = api_server
        self.api_port = api_port
        self.metrics_port = metrics_port
        self.api_base_url = f"http://{api_server}:{api_port}"
        self.running = True
        self.created_record_ids = []
        
        # Enhanced metrics with timing information
        self.metrics = {
            # DNS Query Metrics
            "dns_queries_total": 0,
            "dns_queries_success": 0,
            "dns_queries_nxdomain": 0,
            "dns_queries_servfail": 0,
            "dns_queries_timeout": 0,
            "dns_queries_error": 0,
            "dns_response_time_seconds": [],  # Store recent response times
            "dns_last_success_timestamp": 0,
            "dns_consecutive_failures": 0,
            
            # API Metrics
            "api_requests_total": 0,
            "api_requests_success": 0,
            "api_requests_failed": 0,
            "api_response_time_seconds": [],  # Store recent response times
            "api_last_success_timestamp": 0,
            "api_consecutive_failures": 0,
            
            # Record Management
            "records_created_total": 0,
            "records_updated_total": 0,
            "records_deleted_total": 0,
            "records_active": 0,
        }
        
        # Keep only recent response times (last 100 measurements)
        self.max_response_times = 100
        
        # Domains for testing
        self.existing_domains = [
            "test.example.com",
            "canary-test-1.example.com",
            "canary-test-2.example.com", 
            "canary-test-3.example.com",
            "www.canary-test.example.com",
            "mail.canary-test.example.com",
            "api.canary-test.example.com",
            "db.canary-test.example.com",
        ]
        
        self.nonexistent_domains = [
            "nonexistent-domain-12345.com",
            "fake-website-xyz.net",
            "does-not-exist-abc.org",
            "random-gibberish-domain.info",
            "test-nxdomain-response.example",
        ]
        
        self.query_types = ["A", "AAAA", "MX", "CNAME", "TXT", "NS"]
        
        # Sample records for priming
        self.sample_records = [
            {"name": "canary-test-1.example.com", "ip": "192.168.1.10", "ttl": 300, "record_type": "A", "class": "IN"},
            {"name": "canary-test-2.example.com", "ip": "192.168.1.11", "ttl": 600, "record_type": "A", "class": "IN"},
            {"name": "canary-test-3.example.com", "ip": "10.0.0.10", "ttl": 300, "record_type": "A", "class": "IN"},
            {"name": "www.canary-test.example.com", "record_type": "CNAME", "class": "IN", "value": "canary-test-1.example.com", "ttl": 300},
            {"name": "mail.canary-test.example.com", "ip": "192.168.1.20", "ttl": 300, "record_type": "A", "class": "IN"},
            {"name": "api.canary-test.example.com", "ip": "10.0.0.20", "ttl": 300, "record_type": "A", "class": "IN"},
            {"name": "db.canary-test.example.com", "ip": "10.0.0.30", "ttl": 300, "record_type": "A", "class": "IN"},
        ]

    def signal_handler(self, signum, frame):
        """Handle graceful shutdown on SIGINT/SIGTERM"""
        logger.info("Received shutdown signal, stopping canary...")
        self.running = False

    def get_prometheus_metrics(self) -> str:
        """Generate Prometheus metrics format"""
        now = time.time()
        
        # Calculate response time statistics
        dns_response_times = self.metrics["dns_response_time_seconds"]
        api_response_times = self.metrics["api_response_time_seconds"]
        
        dns_avg_response_time = sum(dns_response_times) / len(dns_response_times) if dns_response_times else 0
        api_avg_response_time = sum(api_response_times) / len(api_response_times) if api_response_times else 0
        
        dns_max_response_time = max(dns_response_times) if dns_response_times else 0
        api_max_response_time = max(api_response_times) if api_response_times else 0
        
        # Calculate availability
        dns_total = self.metrics["dns_queries_total"]
        dns_availability = (self.metrics["dns_queries_success"] / dns_total) if dns_total > 0 else 0
        
        api_total = self.metrics["api_requests_total"]
        api_availability = (self.metrics["api_requests_success"] / api_total) if api_total > 0 else 0
        
        # Time since last success
        dns_time_since_success = now - self.metrics["dns_last_success_timestamp"] if self.metrics["dns_last_success_timestamp"] > 0 else 0
        api_time_since_success = now - self.metrics["api_last_success_timestamp"] if self.metrics["api_last_success_timestamp"] > 0 else 0
        
        metrics = f"""# HELP dns_canary_queries_total Total number of DNS queries sent
# TYPE dns_canary_queries_total counter
dns_canary_queries_total{{server="{self.dns_server}:{self.dns_port}"}} {self.metrics["dns_queries_total"]}

# HELP dns_canary_queries_success_total Number of successful DNS queries
# TYPE dns_canary_queries_success_total counter
dns_canary_queries_success_total{{server="{self.dns_server}:{self.dns_port}"}} {self.metrics["dns_queries_success"]}

# HELP dns_canary_queries_nxdomain_total Number of NXDOMAIN DNS responses
# TYPE dns_canary_queries_nxdomain_total counter
dns_canary_queries_nxdomain_total{{server="{self.dns_server}:{self.dns_port}"}} {self.metrics["dns_queries_nxdomain"]}

# HELP dns_canary_queries_servfail_total Number of SERVFAIL DNS responses
# TYPE dns_canary_queries_servfail_total counter
dns_canary_queries_servfail_total{{server="{self.dns_server}:{self.dns_port}"}} {self.metrics["dns_queries_servfail"]}

# HELP dns_canary_queries_timeout_total Number of DNS query timeouts
# TYPE dns_canary_queries_timeout_total counter
dns_canary_queries_timeout_total{{server="{self.dns_server}:{self.dns_port}"}} {self.metrics["dns_queries_timeout"]}

# HELP dns_canary_queries_error_total Number of DNS query errors
# TYPE dns_canary_queries_error_total counter
dns_canary_queries_error_total{{server="{self.dns_server}:{self.dns_port}"}} {self.metrics["dns_queries_error"]}

# HELP dns_canary_response_time_seconds_avg Average DNS response time in seconds
# TYPE dns_canary_response_time_seconds_avg gauge
dns_canary_response_time_seconds_avg{{server="{self.dns_server}:{self.dns_port}"}} {dns_avg_response_time:.6f}

# HELP dns_canary_response_time_seconds_max Maximum DNS response time in seconds
# TYPE dns_canary_response_time_seconds_max gauge
dns_canary_response_time_seconds_max{{server="{self.dns_server}:{self.dns_port}"}} {dns_max_response_time:.6f}

# HELP dns_canary_availability DNS server availability ratio (0-1)
# TYPE dns_canary_availability gauge
dns_canary_availability{{server="{self.dns_server}:{self.dns_port}"}} {dns_availability:.6f}

# HELP dns_canary_consecutive_failures Number of consecutive DNS failures
# TYPE dns_canary_consecutive_failures gauge
dns_canary_consecutive_failures{{server="{self.dns_server}:{self.dns_port}"}} {self.metrics["dns_consecutive_failures"]}

# HELP dns_canary_time_since_last_success_seconds Time since last successful DNS query
# TYPE dns_canary_time_since_last_success_seconds gauge
dns_canary_time_since_last_success_seconds{{server="{self.dns_server}:{self.dns_port}"}} {dns_time_since_success:.1f}

# HELP api_canary_requests_total Total number of API requests sent
# TYPE api_canary_requests_total counter
api_canary_requests_total{{server="{self.api_server}:{self.api_port}"}} {self.metrics["api_requests_total"]}

# HELP api_canary_requests_success_total Number of successful API requests
# TYPE api_canary_requests_success_total counter
api_canary_requests_success_total{{server="{self.api_server}:{self.api_port}"}} {self.metrics["api_requests_success"]}

# HELP api_canary_requests_failed_total Number of failed API requests
# TYPE api_canary_requests_failed_total counter
api_canary_requests_failed_total{{server="{self.api_server}:{self.api_port}"}} {self.metrics["api_requests_failed"]}

# HELP api_canary_response_time_seconds_avg Average API response time in seconds
# TYPE api_canary_response_time_seconds_avg gauge
api_canary_response_time_seconds_avg{{server="{self.api_server}:{self.api_port}"}} {api_avg_response_time:.6f}

# HELP api_canary_availability API server availability ratio (0-1)
# TYPE api_canary_availability gauge
api_canary_availability{{server="{self.api_server}:{self.api_port}"}} {api_availability:.6f}

# HELP api_canary_consecutive_failures Number of consecutive API failures
# TYPE api_canary_consecutive_failures gauge
api_canary_consecutive_failures{{server="{self.api_server}:{self.api_port}"}} {self.metrics["api_consecutive_failures"]}

# HELP api_canary_time_since_last_success_seconds Time since last successful API request
# TYPE api_canary_time_since_last_success_seconds gauge
api_canary_time_since_last_success_seconds{{server="{self.api_server}:{self.api_port}"}} {api_time_since_success:.1f}

# HELP dns_canary_records_active Number of active test records
# TYPE dns_canary_records_active gauge
dns_canary_records_active {len(self.created_record_ids)}

# HELP dns_canary_up Canary health status (1 = up, 0 = down)
# TYPE dns_canary_up gauge
dns_canary_up 1
"""
        return metrics

    def start_metrics_server(self):
        """Start HTTP server for metrics endpoint"""
        try:
            server = HTTPServer(('0.0.0.0', self.metrics_port), MetricsHandler)
            server.canary = self  # Pass reference to canary for metrics access
            
            def serve_forever():
                logger.info(f"Metrics server started on http://0.0.0.0:{self.metrics_port}/metrics")
                server.serve_forever()
            
            metrics_thread = threading.Thread(target=serve_forever, daemon=True)
            metrics_thread.start()
            return server
        except Exception as e:
            logger.error(f"Failed to start metrics server: {e}")
            return None

    def send_dns_query(self, domain: str, query_type: str) -> Tuple[str, float]:
        """Send DNS query and return response code and duration"""
        start_time = time.time()
        
        try:
            cmd = [
                "dig", 
                f"@{self.dns_server}", 
                "-p", str(self.dns_port),
                "+short",
                "+time=2",
                "+tries=1",
                domain, 
                query_type
            ]
            
            result = subprocess.run(
                cmd, 
                capture_output=True, 
                text=True, 
                timeout=5
            )
            
            duration = time.time() - start_time
            
            if result.returncode == 0:
                return "NOERROR", duration
            elif result.returncode == 9:
                return "NXDOMAIN", duration
            elif result.returncode == 2:
                return "SERVFAIL", duration
            else:
                return "OTHER", duration
                
        except subprocess.TimeoutExpired:
            duration = time.time() - start_time
            return "TIMEOUT", duration
        except Exception as e:
            duration = time.time() - start_time
            logger.error(f"Error sending DNS query: {e}")
            return "ERROR", duration

    def update_dns_metrics(self, response_code: str, duration: float):
        """Update DNS metrics"""
        self.metrics["dns_queries_total"] += 1
        
        # Add response time to recent measurements
        self.metrics["dns_response_time_seconds"].append(duration)
        if len(self.metrics["dns_response_time_seconds"]) > self.max_response_times:
            self.metrics["dns_response_time_seconds"].pop(0)
        
        if response_code == "NOERROR":
            self.metrics["dns_queries_success"] += 1
            self.metrics["dns_last_success_timestamp"] = time.time()
            self.metrics["dns_consecutive_failures"] = 0
        elif response_code == "NXDOMAIN":
            self.metrics["dns_queries_nxdomain"] += 1
            self.metrics["dns_consecutive_failures"] += 1
        elif response_code == "SERVFAIL":
            self.metrics["dns_queries_servfail"] += 1
            self.metrics["dns_consecutive_failures"] += 1
        elif response_code == "TIMEOUT":
            self.metrics["dns_queries_timeout"] += 1
            self.metrics["dns_consecutive_failures"] += 1
        else:
            self.metrics["dns_queries_error"] += 1
            self.metrics["dns_consecutive_failures"] += 1

    def api_request(self, method: str, endpoint: str, data: Optional[Dict] = None) -> Optional[Dict]:
        """Make an API request and update metrics"""
        start_time = time.time()
        
        try:
            url = f"{self.api_base_url}{endpoint}"
            headers = {"Content-Type": "application/json"}
            
            if method == "GET":
                response = requests.get(url, timeout=5)
            elif method == "POST":
                response = requests.post(url, json=data, headers=headers, timeout=5)
            elif method == "PUT":
                response = requests.put(url, json=data, headers=headers, timeout=5)
            elif method == "DELETE":
                response = requests.delete(url, timeout=5)
            else:
                return None
            
            duration = time.time() - start_time
            self.metrics["api_requests_total"] += 1
            
            # Add response time to recent measurements
            self.metrics["api_response_time_seconds"].append(duration)
            if len(self.metrics["api_response_time_seconds"]) > self.max_response_times:
                self.metrics["api_response_time_seconds"].pop(0)
            
            if response.status_code in [200, 201, 204]:
                self.metrics["api_requests_success"] += 1
                self.metrics["api_last_success_timestamp"] = time.time()
                self.metrics["api_consecutive_failures"] = 0
                
                if response.status_code != 204:
                    return response.json()
                return {"success": True}
            else:
                self.metrics["api_requests_failed"] += 1
                self.metrics["api_consecutive_failures"] += 1
                return None
                
        except Exception as e:
            duration = time.time() - start_time
            self.metrics["api_requests_total"] += 1
            self.metrics["api_requests_failed"] += 1
            self.metrics["api_consecutive_failures"] += 1
            
            # Still record response time for failed requests
            self.metrics["api_response_time_seconds"].append(duration)
            if len(self.metrics["api_response_time_seconds"]) > self.max_response_times:
                self.metrics["api_response_time_seconds"].pop(0)
            
            logger.error(f"API request error: {e}")
            return None

    def prime_datastore(self):
        """Prime the datastore with sample records"""
        logger.info("Priming datastore with sample records...")
        
        for record_data in self.sample_records:
            result = self.api_request("POST", "/records", record_data)
            if result and result.get("success"):
                record_id = result["data"]["id"]
                self.created_record_ids.append(record_id)
                self.metrics["records_created_total"] += 1
                logger.info(f"Created sample record: {record_data['name']} (ID: {record_id})")
        
        self.metrics["records_active"] = len(self.created_record_ids)
        logger.info(f"Datastore priming complete. Created {len(self.created_record_ids)} records.")

    def select_query(self) -> Tuple[str, str]:
        """Select a random domain and query type"""
        # 70% existing domains, 30% non-existent
        if random.random() < 0.7:
            domain = random.choice(self.existing_domains)
        else:
            domain = random.choice(self.nonexistent_domains)
        
        query_type = random.choice(self.query_types)
        return domain, query_type

    def run(self, prime_datastore=True, enable_api_ops=True):
        """Main canary loop"""
        logger.info(f"Starting DNS Canary with Metrics - DNS: {self.dns_server}:{self.dns_port}, API: {self.api_server}:{self.api_port}")
        
        # Start metrics server
        metrics_server = self.start_metrics_server()
        if not metrics_server:
            logger.error("Failed to start metrics server, continuing without metrics endpoint")
        
        # Set up signal handlers
        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)
        
        # Prime datastore
        if prime_datastore:
            self.prime_datastore()
        
        last_stats_log = time.time()
        
        while self.running:
            try:
                # Perform DNS query
                domain, query_type = self.select_query()
                response_code, duration = self.send_dns_query(domain, query_type)
                self.update_dns_metrics(response_code, duration)
                
                logger.debug(f"DNS Query: {domain} {query_type} -> {response_code} ({duration:.3f}s)")
                
                # Occasional API operations
                if enable_api_ops and random.random() < 0.05:
                    self.perform_random_api_operation()
                
                # Log stats every 60 seconds
                if time.time() - last_stats_log > 60:
                    self.log_stats()
                    last_stats_log = time.time()
                
                # Short sleep between queries
                time.sleep(random.uniform(0.1, 0.5))
                
            except KeyboardInterrupt:
                break
            except Exception as e:
                logger.error(f"Unexpected error in canary loop: {e}")
                time.sleep(1)
        
        logger.info("DNS Canary with Metrics stopped")

    def perform_random_api_operation(self):
        """Perform a random API operation"""
        operations = ["create", "read", "list"]
        operation = random.choice(operations)
        
        if operation == "create":
            self.create_random_record()
        elif operation == "read" and self.created_record_ids:
            record_id = random.choice(self.created_record_ids)
            self.api_request("GET", f"/records/{record_id}")
        elif operation == "list":
            self.api_request("GET", "/records?page=1&per_page=10")

    def create_random_record(self):
        """Create a random DNS record"""
        record_data = {
            "name": f"canary-{random.randint(1000, 9999)}.test.local",
            "ip": f"192.168.{random.randint(1, 254)}.{random.randint(1, 254)}",
            "ttl": 300,
            "record_type": "A",
            "class": "IN"
        }
        
        result = self.api_request("POST", "/records", record_data)
        if result and result.get("success"):
            record_id = result["data"]["id"]
            self.created_record_ids.append(record_id)
            self.metrics["records_created_total"] += 1

    def log_stats(self):
        """Log current statistics"""
        dns_total = self.metrics["dns_queries_total"]
        api_total = self.metrics["api_requests_total"]
        
        if dns_total > 0:
            success_rate = (self.metrics["dns_queries_success"] / dns_total) * 100
            logger.info(f"DNS Stats - Total: {dns_total}, Success: {success_rate:.1f}%, "
                       f"Consecutive Failures: {self.metrics['dns_consecutive_failures']}")
        
        if api_total > 0:
            success_rate = (self.metrics["api_requests_success"] / api_total) * 100
            logger.info(f"API Stats - Total: {api_total}, Success: {success_rate:.1f}%, "
                       f"Active Records: {len(self.created_record_ids)}")

def main():
    import argparse
    
    parser = argparse.ArgumentParser(description="DNS Canary with Prometheus Metrics")
    parser.add_argument("--dns-server", default="localhost", help="DNS server address")
    parser.add_argument("--dns-port", type=int, default=12312, help="DNS server port")
    parser.add_argument("--api-server", default="localhost", help="API server address")
    parser.add_argument("--api-port", type=int, default=8080, help="API server port")
    parser.add_argument("--metrics-port", type=int, default=8090, help="Metrics server port")
    parser.add_argument("--no-prime", action="store_true", help="Skip datastore priming")
    parser.add_argument("--no-api", action="store_true", help="Disable API operations")
    parser.add_argument("--verbose", "-v", action="store_true", help="Enable verbose logging")
    
    args = parser.parse_args()
    
    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)
    
    canary = DNSCanaryWithMetrics(
        dns_server=args.dns_server,
        dns_port=args.dns_port,
        api_server=args.api_server,
        api_port=args.api_port,
        metrics_port=args.metrics_port
    )
    
    canary.run(
        prime_datastore=not args.no_prime,
        enable_api_ops=not args.no_api
    )

if __name__ == "__main__":
    main()