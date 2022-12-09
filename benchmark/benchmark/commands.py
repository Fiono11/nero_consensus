# Copyright(C) Facebook, Inc. and its affiliates.
from os.path import join

from benchmark.utils import PathMaker


class CommandMaker:

    @staticmethod
    def cleanup():
        return (
            f'rm .*.json ; mkdir -p {PathMaker.results_path()}'
        )

    @staticmethod
    def clean_dbs(i):
        return f'rm -r {PathMaker.db_path(i)} ; mkdir -p {PathMaker.db_path(i)}'

    @staticmethod
    def clean_logs():
        return f'rm -r {PathMaker.logs_path()} ; mkdir -p {PathMaker.logs_path()}'

    @staticmethod
    def compile():
        return 'cargo build --quiet --release --features benchmark'

    @staticmethod
    def generate_key(filename):
        assert isinstance(filename, str)
        return f'./node generate_keys --filename {filename}'

    @staticmethod
    def run_primary(keys, committee, store, parameters, byzantine, debug=True):
        assert isinstance(keys, str)
        assert isinstance(committee, str)
        assert isinstance(parameters, str)
        assert isinstance(byzantine, str)
        assert isinstance(debug, bool)
        v = '-vvv' if debug else '-vv'
        return f'./node {v} run --keys {keys} --committee {committee} --store {store} --parameters {parameters} ' \
               f'--byzantine {byzantine} primary'

    @staticmethod
    def run_client(address, size, rate):
        assert isinstance(address, str)
        assert isinstance(size, int) and size > 0
        assert isinstance(rate, int) and rate >= 0
        return f'./benchmark_client {address} --size {size} --rate {rate}'

    @staticmethod
    def kill():
        return 'tmux kill-server'

    @staticmethod
    def alias_binaries(origin):
        assert isinstance(origin, str)
        node, client = join(origin, 'node'), join(origin, 'benchmark_client')
        return f'rm node ; rm benchmark_client ; ln -s {node} . ; ln -s {client} .'
