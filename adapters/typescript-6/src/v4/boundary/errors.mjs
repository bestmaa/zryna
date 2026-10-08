

class AdapterError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

function failRequest(message) {
  throw new AdapterError('ZRYNA-F1001', message);
}

function failBudget(message) {
  throw new AdapterError('ZRYNA-F1002', message);
}

function failInvariant(message) {
  throw new AdapterError('ZRYNA-F1003', message);
}

export {
  AdapterError,
  failBudget,
  failInvariant,
  failRequest
};
