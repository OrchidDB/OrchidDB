"""One product run: choose the required interface before execution, never retry failures."""
import json
import re


def execution_path(case):
    if '@GraphComputerOnly' in case.get('tags', []):
        return 'crabgraph-computer'
    for step in case['steps']:
        text = step['text']
        if text == 'an unsupported test':
            return 'crabgraph-jvm'
        parameter = re.fullmatch(r'using the parameter \w+ defined as (.+)', text)
        if parameter:
            value = json.loads(parameter[1])
            if value.startswith('c[') or re.search(r'(?:^|[\[,])e\[[^\]]*\](?!\.id)', value) or value == 's[]':
                return 'crabgraph-jvm'
        if text in ('the traversal of', 'the graph initializer of'):
            # Ignore quoted property values when identifying inline JVM callbacks.
            code = re.sub(r"'([^'\\]|\\.)*'|\"([^\"\\]|\\.)*\"", '', step.get('doc', ''))
            if re.search(r'\bLambda\s*\.\s*\w+\s*\(', code):
                return 'crabgraph-jvm'
    return 'crabgraph'


class ProductGremlin:
    def __init__(self, factory):
        self.adapters = {key: factory(key) for key in ('crabgraph', 'crabgraph-jvm', 'crabgraph-computer')}
        self.classpath = self.adapters['crabgraph-jvm'].classpath

    def run(self, case):
        selected = execution_path(case)
        result = self.adapters[selected].run(case)
        return {**result, 'execution_path': selected}

    def close(self):
        for adapter in self.adapters.values():
            adapter.close()
