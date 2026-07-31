import { pairGateway } from '../src/network/pairing';

describe('pairGateway', () => {
  it('posts the documented pair payload and returns a credential', async () => {
    const fetchMock = jest.fn().mockResolvedValue({ ok: true, status: 200, json: async () => ({ token: 'device-token', server_name: 'jcode', server_version: 'v1' }) }) as unknown as typeof fetch;
    await expect(pairGateway('gateway.local:8787', { code: '123456', device_id: 'device', device_name: 'Phone' }, fetchMock)).resolves.toEqual({ gateway: 'http://gateway.local:8787', token: 'device-token', server_name: 'jcode', server_version: 'v1' });
    expect(fetchMock).toHaveBeenCalledWith('http://gateway.local:8787/pair', expect.objectContaining({ method: 'POST', body: JSON.stringify({ code: '123456', device_id: 'device', device_name: 'Phone' }) }));
  });

  it('surfaces a server pairing error', async () => {
    const fetchMock = jest.fn().mockResolvedValue({ ok: false, status: 401, json: async () => ({ error: 'Invalid or expired pairing code' }) }) as unknown as typeof fetch;
    await expect(pairGateway('http://gateway.local', { code: 'bad', device_id: 'device', device_name: 'Phone' }, fetchMock)).rejects.toThrow('Invalid or expired pairing code');
  });
});
