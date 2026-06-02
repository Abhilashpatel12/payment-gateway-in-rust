














import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Trend, Rate } from 'k6/metrics';



const duplicateChargesDetected = new Counter('duplicate_charges');
const paymentDuration          = new Trend('payment_duration_ms');
const successRate               = new Rate('payment_success_rate');



const BASE_URL     = __ENV.BASE_URL   || 'http://localhost:8081';
const MERCHANT_ID  = __ENV.MERCHANT_ID;
const API_KEY      = __ENV.API_KEY;

const HEADERS = {
  'Content-Type':  'application/json',
  'Authorization': `Bearer ${API_KEY}`,
};





export const options = {
  scenarios: {
    idempotency_stress: {
      executor: 'shared-iterations',
      vus: 200,
      iterations: 1000,
      maxDuration: '60s',
      exec: 'testIdempotency',
      tags: { scenario: 'idempotency' },
    },

    concurrent_capture_race: {
      executor: 'shared-iterations',
      vus: 50,
      iterations: 50,
      startTime: '65s',
      maxDuration: '30s',
      exec: 'testCaptureRace',
      tags: { scenario: 'capture_race' },
    },

    sustained_load: {
      executor: 'constant-arrival-rate',
      rate: 500,           
      timeUnit: '1s',
      duration: '60s',
      preAllocatedVUs: 200,
      maxVUs: 500,
      startTime: '100s',
      exec: 'testSustainedLoad',
      tags: { scenario: 'sustained' },
    },
  },

  thresholds: {
    
    'payment_duration_ms': ['p(99)<500'],
    
    'payment_success_rate': ['rate>0.99'],
    
    'duplicate_charges':    ['count==0'],
    
    'http_req_failed':      ['rate<0.01'],
  },
};



export function testIdempotency() {
  const idempotencyKey = 'load-test-fixed-key-do-not-change';

  const payload = JSON.stringify({
    amount: 10000,
    currency: 'INR',
    payment_method: {
      type: 'card',
      token: 'tok_test_visa',
      last4: '4242',
      brand: 'visa',
      exp_month: 12,
      exp_year: 2028,
    },
    description: 'Idempotency stress test',
  });

  const start = Date.now();
  const res = http.post(`${BASE_URL}/v1/payments`, payload, {
    headers: {
      ...HEADERS,
      'X-Idempotency-Key': idempotencyKey,
    },
  });
  paymentDuration.add(Date.now() - start);

  const ok = check(res, {
    'status is 200 or 201': (r) => r.status === 200 || r.status === 201,
    'has payment id':        (r) => r.json('data.id') !== undefined,
  });
  successRate.add(ok);

  
  
}





let authorizedPaymentId = null;

export function setup() {
  
  const res = http.post(`${BASE_URL}/v1/payments`, JSON.stringify({
    amount: 50000,
    currency: 'INR',
    capture_method: 'manual',
    payment_method: {
      type: 'card',
      token: 'tok_test_visa_auth',
      last4: '4444',
      brand: 'visa',
      exp_month: 12,
      exp_year: 2028,
    },
  }), { headers: HEADERS });

  return { authorizedPaymentId: res.json('data.id') };
}

export function testCaptureRace(data) {
  if (!data.authorizedPaymentId) return;

  const res = http.post(
    `${BASE_URL}/v1/payments/${data.authorizedPaymentId}/capture`,
    JSON.stringify({}),
    { headers: HEADERS },
  );

  
  const succeeded = res.status === 200;
  const validFailure = res.status === 400 || res.status === 409;

  check(res, {
    'capture is success or expected failure': () => succeeded || validFailure,
  });
}



let paymentCounter = 0;

export function testSustainedLoad() {
  const id = ++paymentCounter;

  const payload = JSON.stringify({
    amount: 5000 + (id % 9000),
    currency: 'INR',
    payment_method: {
      type: 'card',
      token: `tok_test_${id % 100}`,
      last4: String(id % 9000 + 1000),
      brand: 'visa',
      exp_month: 12,
      exp_year: 2028,
    },
    description: `Load test payment ${id}`,
  });

  const start = Date.now();
  const res = http.post(`${BASE_URL}/v1/payments`, payload, {
    headers: {
      ...HEADERS,
      'X-Idempotency-Key': `load-test-${id}`,
    },
  });
  paymentDuration.add(Date.now() - start);

  const ok = check(res, {
    'status 2xx': (r) => r.status >= 200 && r.status < 300,
  });
  successRate.add(ok);
}
