# Copyright(C) Facebook, Inc. and its affiliates.
import unittest
from datetime import datetime
from glob import glob
from multiprocessing import Pool
from os.path import join
from re import findall, search, split
from statistics import mean
from datetime import datetime

import numpy
from benchmark.utils import Print
from six import assertCountEqual


class ParseError(Exception):
    pass

class LogParser:
    txs = 1

    def __init__(self, clients, primaries, faults=1):
        inputs = [clients, primaries]
        assert all(isinstance(x, list) for x in inputs)
        assert all(isinstance(x, str) for y in inputs for x in y)
        assert all(x for x in inputs)

        self.faults = faults
        if isinstance(faults, int):
            self.committee_size = len(primaries) + int(faults)
        else:
            self.committee_size = '?'

        # Parse the clients logs.
        try:
            with Pool() as p:
                results = p.map(self._parse_clients, clients)
        except (ValueError, IndexError, AttributeError) as e:
            raise ParseError(f'Failed to parse clients\' logs: {e}')
        self.size, self.rate, self.start, misses, self.sent_samples \
            = zip(*results)
        self.misses = sum(misses)

        # Parse the primaries logs.
        try:
            with Pool() as p:
                results = p.map(self._parse_primaries, primaries)
        except (ValueError, IndexError, AttributeError) as e:
            raise ParseError(f'Failed to parse nodes\' logs: {e}')
        proposals, commits, decisions = zip(*results)
        self.proposals = self._merge_results([x.items() for x in proposals])
        self.commits = self._merge_results([x.items() for x in commits])

        print(decisions)

        p = len(primaries)

        l = numpy.array_split(decisions, p)

        print(l)

        print(p)

        #print("l1: ", l[0][0])
        #print("l2: ", l[1][0])

        print(sorted(l[0][0]))

        print("SIZE: ", len(sorted(l[0][0])))

        assert len(sorted(l[0][0])) == self.txs # number of txs

        empty = list();

        for i in range(1, p):
            if sorted(l[i][0]):
                print(sorted(l[i][0]))
                #assert sorted(l[0][0]) == sorted(l[i][0])
                assert set(l[0][0]) == set(l[i][0])
            else:
                empty.append(l[i][0])
        assert len(empty) == 1

        # Check whether clients missed their target rate.
        if self.misses != 0:
            Print.warn(
                f'Clients missed their target rate {self.misses:,} time(s)'
            )

        # Check whether clients missed their target rate.
        if self.misses != 0:
            Print.warn(
                f'Clients missed their target rate {self.misses:,} time(s)'
            )

    def _merge_results(self, input):
        # Keep the earliest timestamp.
        merged = {}
        for x in input:
            for k, v in x:
                if not k in merged or merged[k] > v:
                    merged[k] = v
        return merged

    def _parse_clients(self, log):
        if search(r'Error', log) is not None:
            raise ParseError('Client(s) panicked')

        size = int(search(r'Transactions size: (\d+)', log).group(1))
        rate = int(search(r'Transactions rate: (\d+)', log).group(1))

        tmp = search(r'\[(.*Z) .* Start ', log).group(1)
        start = self._to_posix(tmp)

        misses = len(findall(r'rate too high', log))

        tmp = findall(r'\[(.*Z) .* sample transaction (\d+)', log)
        samples = {int(s): self._to_posix(t) for t, s in tmp}

        return size, rate, start, misses, samples

    def _parse_primaries(self, log):
        if search(r'(?:panicked|Error)', log) is not None:
            raise ParseError('Primary(s) panicked')

        tmp = findall(r'\[(.*Z) .* Created ([^ ]+=)', log)
        tmp = [(d, self._to_posix(t)) for t, d in tmp]
        proposals = self._merge_results([tmp])

        tmp = findall(r'\[(.*Z) .* Decided ([^ ]+) -> ([^ ]+=)', log)
        #print(tmp)
        decisions = [(d, a) for t, a, d in tmp]
        #print(decisions)
        tmp = [(d, self._to_posix(t)) for t, a, d in tmp]
        commits = self._merge_results([tmp])

        return proposals, commits, decisions

    def _to_posix(self, string):
        x = datetime.fromisoformat(string.replace('Z', '+00:00'))
        return datetime.timestamp(x)

    def _consensus_throughput(self):
        if not self.commits:
            return 0, 0, 0
        start, end = min(self.proposals.values()), max(self.commits.values())
        duration = end - start
        #bytes = sum(self.sizes.values())
        # total bytes of all txs
        bytes = 512 * self.txs
        bps = bytes / duration
        tps = bps / self.size[0]
        return tps, bps, duration

    def _consensus_latency(self):
        latency = [c - self.proposals[d] for d, c in self.commits.items()]
        return mean(latency) if latency else 0

    def _end_to_end_throughput(self):
        if not self.commits:
            return 0, 0, 0
        start, end = min(self.start), max(self.commits.values())
        #print("commits: ", end)
        duration = end - start
        #bytes = sum(self.sizes.values())
        #bps = bytes / duration
        #tps = bps / self.size[0]
        bps = 0
        tps = self.sizes / duration
        return tps, bps, duration

    def _end_to_end_latency(self):
        latency = []
        for sent, received in zip(self.sent_samples, self.received_samples):
            for tx_id, batch_id in received.items():
                if batch_id in self.commits:
                    assert tx_id in sent  # We receive txs that we sent.
                    start = sent[tx_id]
                    end = self.commits[batch_id]
                    latency += [end-start]
        return mean(latency) if latency else 0

    def result(self):
        #sync_retry_delay = self.configs[0]['sync_retry_delay']
        #sync_retry_nodes = self.configs[0]['sync_retry_nodes']
        #batch_size = self.configs[0]['batch_size']
        #max_batch_delay = self.configs[0]['max_batch_delay']

        consensus_latency = self._consensus_latency() * 1_000
        consensus_tps, consensus_bps, duration = self._consensus_throughput()
        #end_to_end_tps, end_to_end_bps, duration = self._end_to_end_throughput()
        #end_to_end_latency = self._end_to_end_latency() * 1_000

        return (
            '\n'
            '-----------------------------------------\n'
            ' SUMMARY:\n'
            '-----------------------------------------\n'
            ' + CONFIG:\n'
            f' Faults: {self.faults} node(s)\n'
            f' Committee size: {self.committee_size} node(s)\n'
            f' Input rate: {sum(self.rate):,} tx/s\n'
            f' Transaction size: {self.size[0]:,} B\n'
            f' Execution time: {round(duration):,} s\n'
            '\n'
            #f' Sync retry delay: {sync_retry_delay:,} ms\n'
            #f' Sync retry nodes: {sync_retry_nodes:,} node(s)\n'
            #f' batch size: {batch_size:,} B\n'
            #f' Max batch delay: {max_batch_delay:,} ms\n'
            '\n'
            ' + RESULTS:\n'
            f' Consensus TPS: {round(consensus_tps):,} tx/s\n'
            f' Consensus BPS: {round(consensus_bps):,} B/s\n'
            f' Consensus latency: {consensus_latency:,} ms\n'
            #'\n'
            #f' End-to-end TPS: {round(end_to_end_tps):,} tx/s\n'
            #f' End-to-end BPS: {round(end_to_end_bps):,} B/s\n'
            #f' End-to-end latency: {round(end_to_end_latency):,} ms\n'
            '-----------------------------------------\n'
        )

    def print(self, filename):
        assert isinstance(filename, str)
        with open(filename, 'a') as f:
            f.write(self.result())

    @classmethod
    def process(cls, directory, faults=0):
        assert isinstance(directory, str)

        clients = []
        for filename in sorted(glob(join(directory, 'client-*.log'))):
            with open(filename, 'r') as f:
                clients += [f.read()]
        primaries = []
        for filename in sorted(glob(join(directory, 'primary-*.log'))):
            with open(filename, 'r') as f:
                primaries += [f.read()]

        return cls(clients, primaries, faults=faults)
