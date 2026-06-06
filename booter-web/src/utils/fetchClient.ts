import { toast } from 'sonner';

export async function fetchClient(input: RequestInfo | URL, init?: RequestInit): Promise<any> {
    const token = localStorage.getItem('booter_token');
    
    let modifiedInit = init || {};
    if (token) {
        modifiedInit.headers = {
            ...modifiedInit.headers,
            'Authorization': `Bearer ${token}`
        };
    }

    try {
        const response = await fetch(input, modifiedInit);
        
        if (response.status === 401) {
            localStorage.removeItem('booter_token');
            localStorage.removeItem('booter_role');
            if (window.location.pathname !== '/login' && window.location.pathname !== '/admin/login') {
                window.location.href = '/login';
            }
            throw new Error("会话已过期，请重新登录");
        }
        
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
