# Copyright(C) Facebook, Inc. and its affiliates.
import subprocess
from math import ceil
from os.path import basename, splitext
from time import sleep

from benchmark.commands import CommandMaker
from benchmark.config import Key, LocalCommittee, NodeParameters, BenchParameters, ConfigError

from benchmark.logs import LogParser, ParseError
from benchmark.utils import Print, BenchError, PathMaker

class LocalBench:
    BASE_PORT = 3000

    def __init__(self, bench_parameters_dict, node_parameters_dict):
        try:
            self.bench_parameters = BenchParameters(bench_parameters_dict)
            self.node_parameters = NodeParameters(node_parameters_dict)
        except ConfigError as e:
            raise BenchError('Invalid nodes or bench parameters', e)

    def __getattr__(self, attr):
        return getattr(self.bench_parameters, attr)

    def _background_run(self, command, log_file):
        name = splitext(basename(log_file))[0]
        cmd = f'{command} 2> {log_file}'
        subprocess.run(['tmux', 'new', '-d', '-s', name, cmd], check=True)

    def _kill_nodes(self):
        try:
            cmd = CommandMaker.kill().split()
            subprocess.run(cmd, stderr=subprocess.DEVNULL)
        except subprocess.SubprocessError as e:
            raise BenchError('Failed to kill testbed', e)

    def run(self, debug=True):
        assert isinstance(debug, bool)
        Print.heading('Starting local benchmark')

        # Kill any previous testbed.
        self._kill_nodes()

        try:
            Print.info('Setting up testbed...')
            nodes, rate = self.nodes[0], self.rate[0]

            # Cleanup all files.
            cmd = f'{CommandMaker.clean_logs()} ; {CommandMaker.cleanup()}'
            subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)
            sleep(0.5)  # Removing the store may take time.

            # Recompile the latest code.
            cmd = CommandMaker.compile().split()
            subprocess.run(cmd, check=True, cwd=PathMaker.node_crate_path())

            # Create alias for the client and nodes binary.
            cmd = CommandMaker.alias_binaries(PathMaker.binary_path())
            subprocess.run([cmd], shell=True)

            # Generate configuration files.
            keys = []
            key_files = [PathMaker.key_file(i) for i in range(nodes)]
            for filename in key_files:
                cmd = CommandMaker.generate_key(filename).split()
                subprocess.run(cmd, check=True)
                keys += [Key.from_file(filename)]

            names = [x.name for x in keys]
            print(names)
            committee = LocalCommittee(names, self.BASE_PORT)
            committee.print(PathMaker.committee_file())

            self.node_parameters.print(PathMaker.parameters_file())

            # Run the clients (they will wait for the nodes to be ready).
            primary_addresses = committee.transactions(self.faults)
            print(primary_addresses)
            rate_share = rate

            for i, address in enumerate(committee.primary_addresses(self.faults)):
                cmd = f'{CommandMaker.clean_dbs(i)}'
                subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)
                sleep(0.5)  # Removing the store may take time.

            # Run the primaries (except the faulty ones).
            addresses = committee.primary_addresses(self.faults)
            number_of_nodes = len(addresses)
            number_of_byzantine_nodes = (number_of_nodes - 1) / 3;
            number_of_honest_nodes = number_of_nodes - number_of_byzantine_nodes
            for i, address in enumerate(addresses):
                if i < number_of_honest_nodes:
                    cmd = CommandMaker.run_primary(
                        PathMaker.key_file(i),
                        PathMaker.committee_file(),
                        PathMaker.db_path(i),
                        PathMaker.parameters_file(),
                        'false',
                        debug=debug
                    )
                else:
                    cmd = CommandMaker.run_primary(
                        PathMaker.key_file(i),
                        PathMaker.committee_file(),
                        PathMaker.db_path(i),
                        PathMaker.parameters_file(),
                        'true',
                        debug=debug
                    )
                log_file = PathMaker.primary_log_file(i)
                self._background_run(cmd, log_file)

            # Run the clients (they will wait for the nodes to be ready).
            for i, address in enumerate(primary_addresses):
                print(address)
                cmd = CommandMaker.run_client(
                    address,
                    self.tx_size,
                    rate_share,
                )
                log_file = PathMaker.client_log_file(i)
                self._background_run(cmd, log_file)

            # Wait for all transactions to be processed.
            Print.info(f'Running benchmark ({self.duration} sec)...')
            sleep(self.duration)
            self._kill_nodes()

            # Parse logs and return the parser.
            Print.info('Parsing logs...')

            logger = LogParser.process(PathMaker.logs_path(), faults=self.faults)
            logger.print(PathMaker.result_file(
                self.faults,
                4,
                1,
                512,
            ))

            return LogParser.process(PathMaker.logs_path(), faults=self.faults)

        except (subprocess.SubprocessError, ParseError) as e:
            self._kill_nodes()
            raise BenchError('Failed to run benchmark', e)
