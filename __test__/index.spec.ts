import test from 'ava'

test('sync function from native code', (t) => {
  const fixture = 42
  t.is(42 + 100, fixture + 100)
})
