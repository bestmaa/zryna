"""Bounded reader for this pinned Rust Debug observation, not a canonical IR protocol."""
import json
import re

TOKEN = re.compile(r'\s*(?:([A-Za-z_][A-Za-z_0-9]*)|(-?\d+)|("(?:[^"\\]|\\.)*")|(.))', re.S)

class Reader:
    def __init__(self, text):
        self.tokens = []
        offset = 0
        while offset < len(text):
            match = TOKEN.match(text, offset)
            if not match:
                raise ValueError('invalid Debug token')
            token = next(value for value in match.groups() if value is not None)
            self.tokens.append(token)
            offset = match.end()
        self.position = 0
        self.nodes = 0
    def peek(self):
        return self.tokens[self.position] if self.position < len(self.tokens) else None
    def take(self, expected=None):
        token = self.peek()
        if token is None or expected is not None and token != expected:
            raise ValueError('invalid Debug punctuation: ' + str(expected))
        self.position += 1
        return token
    def string(self, token):
        # Rust string Debug differs from JSON only in the bounded escape forms handled here.
        token = re.sub(r'\\u\{([0-9a-fA-F]+)\}', lambda m: chr(int(m[1],16)), token)
        token = token.replace(r'\0', r'\u0000')
        token = re.sub(r'\\x([0-9a-fA-F]{2})', lambda m: '\\u00'+m[1], token)
        return json.loads(token)
    def sequence(self, close, depth):
        values = []
        while self.peek() != close:
            values.append(self.value(depth+1))
            if self.peek() == close:
                break
            self.take(',')
        self.take(close)
        return values
    def value(self, depth=0):
        self.nodes += 1
        if depth > 256 or self.nodes > 500000:
            raise ValueError('bounded Debug structure')
        token = self.take()
        if token == '[':
            return self.sequence(']', depth)
        if token == '(':
            return {'@tuple':self.sequence(')',depth)}
        if token == '{':
            pairs = []
            while self.peek() != '}':
                key = self.value(depth+1)
                self.take(':')
                pairs.append([key,self.value(depth+1)])
                if self.peek() == '}':
                    break
                self.take(',')
            self.take('}')
            return {'@map':pairs}
        if token.startswith('"'):
            return self.string(token)
        if re.fullmatch(r'-?\d+',token):
            return int(token)
        if token in ('true','false'):
            return token == 'true'
        if not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*',token):
            raise ValueError('invalid Debug value: '+token)
        if self.peek() == '(':
            self.take('(')
            return {'@tag':token,'@args':self.sequence(')',depth)}
        if self.peek() == '{':
            self.take('{')
            fields = {}
            while self.peek() != '}':
                name = self.take()
                if not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9]*',name) or name in fields:
                    raise ValueError('duplicate or invalid Debug field')
                self.take(':')
                fields[name] = self.value(depth+1)
                if self.peek() == '}':
                    break
                self.take(',')
            self.take('}')
            return {'@tag':token,'@fields':fields}
        return {'@tag':token}

def parse(text):
    reader = Reader(text)
    value = reader.value()
    if reader.peek() is not None:
        raise ValueError('trailing Debug input')
    return value

def fields(value, tag=None):
    if type(value) is not dict or '@fields' not in value or tag and value.get('@tag') != tag:
        raise ValueError('expected sealed Debug struct: '+str(tag))
    return value['@fields']

def argument(value):
    if type(value) is not dict or len(value.get('@args', [])) != 1:
        raise ValueError('expected one sealed Debug argument')
    return value['@args'][0]
