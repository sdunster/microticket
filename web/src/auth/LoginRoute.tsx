import { useNavigate } from "react-router";
import { setSessionToken } from "../lib/sessionToken";
import LoginPage from "./LoginPage";

/** The standalone `/login` route — one of the home page's two doors. */
export default function LoginRoute() {
  const navigate = useNavigate();
  return (
    <LoginPage
      onNewTokenReceived={(token) => {
        setSessionToken(token);
        navigate("/app", { replace: true });
      }}
    />
  );
}
