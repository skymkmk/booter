import { toast } from 'sonner';

export async function fetchClient(input: RequestInfo | URL, init?: RequestInit): Promise<any> {
    try {
        const response = await fetch(input, init);
        
        let data = null;
        try {
            data = await response.json();
        } catch (e) {
            // response may not be JSON
        }

        if (!response.ok) {
            const errorMsg = data?.message || data?.error || `请求失败 (${response.status})`;
            toast.error(errorMsg);
            throw new Error(errorMsg);
        }

        if (data && data.success === false) {
            const errorMsg = data.message || "操作失败";
            toast.error(errorMsg);
            throw new Error(errorMsg);
        }

        // Return the parsed data if available, otherwise just return the response
        return data !== null ? data : response;
    } catch (err: any) {
        if (err.name === 'TypeError') {
            toast.error("网络请求失败，请检查连接");
        }
        throw err;
    }
}
