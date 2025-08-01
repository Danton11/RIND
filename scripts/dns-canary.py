#!/usr/bin/env python3
"""
DNS Canary Script - Generates realistic DNS traffic and API operations for testing and monitoring

This script performs:
- DNS queries to the RIND DNS server with random domains and query types
- API operations (CREATE, READ, UPDATE, DELETE) on the REST API
- Datastore priming with initial test records
- Periodic API health checks and operations
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

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler('dns-canary.log'),
        logging.StreamHandler(sys.stdout)
    ]
)
logger = logging.getLogger(__name__)

class DNSCanary:
    def __init__(self, dns_server="localhost", dns_port=12312, api_server="localhost", api_port=8080):
        self.dns_server = dns_server
        self.dns_port = dns_port
        self.api_server = api_server
        self.api_port = api_port
        self.api_base_url = f"http://{api_server}:{api_port}"
        self.running = True
        self.created_record_ids = []  # Track records we create for cleanup
        
        # Domains that should exist in the DNS server (based on actual dns_records.txt)
        self.existing_domains = [
            "test.example.com",
            "canary-test-1.example.com",
            "canary-test-2.example.com", 
            "canary-test-3.example.com",
            "www.canary-test.example.com",
            "mail.canary-test.example.com",
            "api.canary-test.example.com",
            "db.canary-test.example.com",
            "test-canary.example.com",
            "minimal.example.com",
            "minimal2.example.com"
        ]
        
        # Random domains that likely don't exist
        self.nonexistent_domains = [
            "nonexistent-domain-12345.com",
            "fake-website-xyz.net",
            "does-not-exist-abc.org",
            "random-gibberish-domain.info",
            "test-nxdomain-response.example",
            "missing-record.test",
            "invalid-hostname-999.local"
        ]
        
        # DNS query types to test
        self.query_types = ["A", "AAAA", "MX", "CNAME", "TXT", "NS"]
        
        # Query patterns with expected responses
        self.query_patterns = [
            # Existing domains - should return NOERROR
            {"domains": self.existing_domains, "weight": 60, "expected": "NOERROR"},
            # Non-existent domains - should return NXDOMAIN  
            {"domains": self.nonexistent_domains, "weight": 30, "expected": "NXDOMAIN"},
            # Edge cases that might cause SERVFAIL
            {"domains": ["", ".", "invalid..domain", "toolong" + "x" * 250 + ".com"], "weight": 10, "expected": "SERVFAIL"}
        ]
        
        # Statistics tracking
        self.stats = {
            "total_queries": 0,
            "noerror_responses": 0,
            "nxdomain_responses": 0,
            "servfail_responses": 0,
            "timeout_responses": 0,
            "other_responses": 0,
            "api_operations": 0,
            "api_successes": 0,
            "api_failures": 0,
            "records_created": 0,
            "records_updated": 0,
            "records_deleted": 0
        }
        
        # Sample data for priming the datastore
        self.sample_records = [
            {"name": "canary-test-1.example.com", "ip": "192.168.1.10", "ttl": 300, "record_type": "A", "class": "IN"},
            {"name": "canary-test-2.example.com", "ip": "192.168.1.11", "ttl": 600, "record_type": "A", "class": "IN"},
            {"name": "canary-test-3.example.com", "ip": "10.0.0.10", "ttl": 300, "record_type": "A", "class": "IN"},
            {"name": "www.canary-test.example.com", "record_type": "CNAME", "class": "IN", "value": "canary-test-1.example.com", "ttl": 300},
            {"name": "mail.canary-test.example.com", "ip": "192.168.1.20", "ttl": 300, "record_type": "A", "class": "IN"},
            {"name": "canary-test.example.com", "record_type": "TXT", "class": "IN", "value": "v=spf1 include:_spf.google.com ~all", "ttl": 300},
            {"name": "api.canary-test.example.com", "ip": "10.0.0.20", "ttl": 300, "record_type": "A", "class": "IN"},
            {"name": "db.canary-test.example.com", "ip": "10.0.0.30", "ttl": 300, "record_type": "A", "class": "IN"},
        ]

    def signal_handler(self, signum, frame):
        """Handle graceful shutdown on SIGINT/SIGTERM"""
        logger.info("Received shutdown signal, stopping canary...")
        self.running = False

    def api_request(self, method: str, endpoint: str, data: Optional[Dict] = None) -> Optional[Dict]:
        """Make an API request to the RIND server"""
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
                logger.error(f"Unsupported HTTP method: {method}")
                return None
            
            self.stats["api_operations"] += 1
            
            if response.status_code in [200, 201, 204]:
                self.stats["api_successes"] += 1
                if response.status_code != 204:  # No content for DELETE
                    return response.json()
                return {"success": True}
            else:
                self.stats["api_failures"] += 1
                logger.warning(f"API request failed: {method} {endpoint} -> {response.status_code}")
                return None
                
        except requests.exceptions.RequestException as e:
            self.stats["api_failures"] += 1
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
                self.stats["records_created"] += 1
                logger.info(f"Created sample record: {record_data['name']} (ID: {record_id})")
            else:
                logger.warning(f"Failed to create sample record: {record_data['name']}")
        
        logger.info(f"Datastore priming complete. Created {len(self.created_record_ids)} records.")

    def perform_api_operations(self):
        """Perform random API operations"""
        operations = ["create", "read", "update", "delete", "list"]
        operation = random.choice(operations)
        
        try:
            if operation == "create":
                self.create_random_record()
            elif operation == "read":
                self.read_random_record()
            elif operation == "update":
                self.update_random_record()
            elif operation == "delete":
                self.delete_random_record()
            elif operation == "list":
                self.list_records()
        except Exception as e:
            logger.error(f"Error performing API operation {operation}: {e}")

    def create_random_record(self):
        """Create a random DNS record via API"""
        domains = [
            f"dynamic-{random.randint(1000, 9999)}.example.com",
            f"test-{uuid.uuid4().hex[:8]}.canary.local",
            f"api-test-{random.randint(100, 999)}.test.org"
        ]
        
        record_types = ["A", "CNAME", "TXT"]
        record_type = random.choice(record_types)
        
        record_data = {
            "name": random.choice(domains),
            "ttl": random.choice([300, 600, 900, 1800]),
            "record_type": record_type,
            "class": "IN"
        }
        
        if record_type == "A":
            record_data["ip"] = f"192.168.{random.randint(1, 254)}.{random.randint(1, 254)}"
        elif record_type == "CNAME":
            record_data["value"] = "target.example.com"
        elif record_type == "TXT":
            record_data["value"] = f"test-value-{random.randint(1, 1000)}"
        
        result = self.api_request("POST", "/records", record_data)
        if result and result.get("success"):
            record_id = result["data"]["id"]
            self.created_record_ids.append(record_id)
            self.stats["records_created"] += 1
            logger.debug(f"Created record: {record_data['name']} (ID: {record_id})")

    def read_random_record(self):
        """Read a random record via API"""
        if not self.created_record_ids:
            return
        
        record_id = random.choice(self.created_record_ids)
        result = self.api_request("GET", f"/records/{record_id}")
        if result and result.get("success"):
            logger.debug(f"Read record: {result['data']['name']} (ID: {record_id})")

    def update_random_record(self):
        """Update a random record via API"""
        if not self.created_record_ids:
            return
        
        record_id = random.choice(self.created_record_ids)
        
        # Random update data
        update_data = {}
        if random.choice([True, False]):
            update_data["ttl"] = random.choice([300, 600, 900, 1800])
        if random.choice([True, False]):
            update_data["ip"] = f"10.0.{random.randint(1, 254)}.{random.randint(1, 254)}"
        
        if update_data:
            result = self.api_request("PUT", f"/records/{record_id}", update_data)
            if result and result.get("success"):
                self.stats["records_updated"] += 1
                logger.debug(f"Updated record: {record_id}")

    def delete_random_record(self):
        """Delete a random record via API"""
        if len(self.created_record_ids) <= len(self.sample_records):
            # Keep at least the sample records
            return
        
        record_id = random.choice(self.created_record_ids)
        result = self.api_request("DELETE", f"/records/{record_id}")
        if result:
            self.created_record_ids.remove(record_id)
            self.stats["records_deleted"] += 1
            logger.debug(f"Deleted record: {record_id}")

    def list_records(self):
        """List records via API with pagination"""
        page = random.randint(1, 3)
        per_page = random.choice([10, 25, 50])
        
        result = self.api_request("GET", f"/records?page={page}&per_page={per_page}")
        if result and result.get("success"):
            total = result["data"]["total"]
            count = len(result["data"]["records"])
            logger.debug(f"Listed records: page {page}, {count} records, {total} total")

    def select_query(self) -> Tuple[str, str]:
        """Select a random domain and query type based on weighted patterns"""
        # Choose pattern based on weights
        total_weight = sum(p["weight"] for p in self.query_patterns)
        rand = random.randint(1, total_weight)
        
        cumulative = 0
        selected_pattern = None
        for pattern in self.query_patterns:
            cumulative += pattern["weight"]
            if rand <= cumulative:
                selected_pattern = pattern
                break
        
        # Select random domain from the chosen pattern
        domain = random.choice(selected_pattern["domains"])
        query_type = random.choice(self.query_types)
        
        return domain, query_type

    def send_dns_query(self, domain: str, query_type: str) -> str:
        """Send DNS query using dig and return the response code"""
        try:
            cmd = [
                "dig", 
                f"@{self.dns_server}", 
                "-p", str(self.dns_port),
                "+short",
                "+time=2",  # 2 second timeout
                "+tries=1", # Only try once
                domain, 
                query_type
            ]
            
            # Run dig command
            result = subprocess.run(
                cmd, 
                capture_output=True, 
                text=True, 
                timeout=5
            )
            
            # Parse response code from dig output
            if result.returncode == 0:
                return "NOERROR"
            elif result.returncode == 9:  # NXDOMAIN
                return "NXDOMAIN"
            elif result.returncode == 2:   # SERVFAIL
                return "SERVFAIL"
            else:
                return "OTHER"
                
        except subprocess.TimeoutExpired:
            return "TIMEOUT"
        except Exception as e:
            logger.error(f"Error sending DNS query: {e}")
            return "ERROR"

    def update_stats(self, response_code: str):
        """Update statistics based on response code"""
        self.stats["total_queries"] += 1
        
        if response_code == "NOERROR":
            self.stats["noerror_responses"] += 1
        elif response_code == "NXDOMAIN":
            self.stats["nxdomain_responses"] += 1
        elif response_code == "SERVFAIL":
            self.stats["servfail_responses"] += 1
        elif response_code == "TIMEOUT":
            self.stats["timeout_responses"] += 1
        else:
            self.stats["other_responses"] += 1

    def log_stats(self):
        """Log current statistics"""
        dns_total = self.stats["total_queries"]
        api_total = self.stats["api_operations"]
        
        if dns_total == 0 and api_total == 0:
            return
        
        # DNS Statistics
        if dns_total > 0:
            logger.info(f"DNS Stats - Total: {dns_total}, "
                       f"NOERROR: {self.stats['noerror_responses']} ({self.stats['noerror_responses']/dns_total*100:.1f}%), "
                       f"NXDOMAIN: {self.stats['nxdomain_responses']} ({self.stats['nxdomain_responses']/dns_total*100:.1f}%), "
                       f"SERVFAIL: {self.stats['servfail_responses']} ({self.stats['servfail_responses']/dns_total*100:.1f}%), "
                       f"TIMEOUT: {self.stats['timeout_responses']} ({self.stats['timeout_responses']/dns_total*100:.1f}%)")
        
        # API Statistics
        if api_total > 0:
            success_rate = self.stats['api_successes'] / api_total * 100 if api_total > 0 else 0
            logger.info(f"API Stats - Total: {api_total}, "
                       f"Success: {self.stats['api_successes']} ({success_rate:.1f}%), "
                       f"Failures: {self.stats['api_failures']}, "
                       f"Created: {self.stats['records_created']}, "
                       f"Updated: {self.stats['records_updated']}, "
                       f"Deleted: {self.stats['records_deleted']}, "
                       f"Active Records: {len(self.created_record_ids)}")

    def run(self, prime_datastore=True, enable_api_ops=True, cleanup_on_exit=True):
        """Main canary loop"""
        logger.info(f"Starting DNS Canary - DNS: {self.dns_server}:{self.dns_port}, API: {self.api_server}:{self.api_port}")
        
        # Set up signal handlers for graceful shutdown
        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)
        
        # Prime datastore with sample records
        if prime_datastore:
            self.prime_datastore()
        
        last_stats_log = time.time()
        last_api_operation = time.time()
        operation_counter = 0
        
        while self.running:
            try:
                # Decide whether to perform DNS query or API operation
                # 95% DNS queries, 5% API operations (much more DNS focused)
                if enable_api_ops and random.random() < 0.05:
                    # Perform API operation
                    self.perform_api_operations()
                    time.sleep(random.uniform(0.1, 0.3))  # Much shorter sleep after API ops
                else:
                    # Perform DNS query
                    domain, query_type = self.select_query()
                    
                    start_time = time.time()
                    response_code = self.send_dns_query(domain, query_type)
                    duration = time.time() - start_time
                    
                    self.update_stats(response_code)
                    logger.debug(f"Query: {domain} {query_type} -> {response_code} ({duration:.3f}s)")
                    
                    # Very short sleep between DNS queries for high load
                    time.sleep(random.uniform(0.001, 0.005))
                
                operation_counter += 1
                
                # Periodic API operations every 60-120 seconds (less frequent)
                if enable_api_ops and time.time() - last_api_operation > random.uniform(60, 120):
                    self.perform_api_operations()
                    last_api_operation = time.time()
                
                # Log stats every 60 seconds
                if time.time() - last_stats_log > 60:
                    self.log_stats()
                    last_stats_log = time.time()
                
            except KeyboardInterrupt:
                break
            except Exception as e:
                logger.error(f"Unexpected error in canary loop: {e}")
                time.sleep(1)
        
        # Cleanup: optionally delete created records
        if cleanup_on_exit:
            logger.info("Cleaning up created records...")
            cleanup_count = 0
            for record_id in self.created_record_ids[:]:  # Copy list to avoid modification during iteration
                if self.api_request("DELETE", f"/records/{record_id}"):
                    cleanup_count += 1
            
            logger.info(f"DNS Canary stopped. Cleaned up {cleanup_count} records.")
        else:
            logger.info(f"DNS Canary stopped. Preserving {len(self.created_record_ids)} created records (cleanup disabled).")
        
        self.log_stats()

def main():
    """Main entry point"""
    import argparse
    
    parser = argparse.ArgumentParser(description="DNS Canary - Generate realistic DNS traffic and API operations")
    parser.add_argument("--dns-server", default="localhost", help="DNS server address (default: localhost)")
    parser.add_argument("--dns-port", type=int, default=12312, help="DNS server port (default: 12312)")
    parser.add_argument("--api-server", default="localhost", help="API server address (default: localhost)")
    parser.add_argument("--api-port", type=int, default=8080, help="API server port (default: 8080)")
    parser.add_argument("--no-prime", action="store_true", help="Skip datastore priming")
    parser.add_argument("--no-api", action="store_true", help="Disable API operations")
    parser.add_argument("--no-cleanup", action="store_true", help="Preserve created records on exit")
    parser.add_argument("--verbose", "-v", action="store_true", help="Enable verbose logging")
    
    args = parser.parse_args()
    
    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)
    
    canary = DNSCanary(
        dns_server=args.dns_server, 
        dns_port=args.dns_port,
        api_server=args.api_server,
        api_port=args.api_port
    )
    canary.run(
        prime_datastore=not args.no_prime,
        enable_api_ops=not args.no_api,
        cleanup_on_exit=not args.no_cleanup
    )

if __name__ == "__main__":
    main()